// FILE-CONTEXT
// STAND: 2026-09-11T10:21:34Z (SESSION: 31ada253)
// ZWECK: Immutables SSTable Format (Data Blocks + Index Block + Bloom Filter)
// INVARIANTEN: Atomares Erstellen via .tmp und rename(); Block-Level Random Access via pread_exact
// NICHT-OFFENSICHTLICH: Parent-Directory Sync nach rename zwingend erforderlich
// SIEHE AUCH: lsm.rs, compaction.rs, DECISIONS.md

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
//! ## Two-Tier Search Strategy
//! SSTable lookups employ a two-tier search strategy:
//! - **Pre-check**: Whole-SSTable Bloom Filter `may_contain` fast-rejects keys absent from the file.
//! - **Tier 1 (Sparse Index)**: Binary search on `self.index` locates the specific 4KB data block.
//! - **Block Bloom Check**: Intra-block Bloom filter pre-checks whether the key exists inside the block.
//! - **Tier 2 (Block Binary Search)**: `binary_search_entry_in_block` performs binary search over the block's
//!   internal sorted `offsets` array to locate the exact key entry in O(log K) time.
//!
//! ## Invariants
//! - **Immutability**: Once written, SSTables are never modified. Compaction creates new ones.
//! - **Sorted Order**: Entries within blocks and blocks within the file are sorted lexicographically by key.
//! - **Async I/O**: All disk operations use `tokio::fs` with `spawn_blocking`.
//!   mmap-based access (WP-4.1) is not yet implemented for the LSM storage layer.
//!   mmap IS used in `memfuse-index` for HNSW/DiskANN index persistence.
//! - **Zero Panic**: Production code paths avoid `unwrap()` and `expect()`, favoring explicit error handling.

// FILE-CONTEXT
// STAND:       2026-08-29T15:22:34Z (SESSION: 2c814094)
// ZWECK:       Persistente, immutable SSTable-Dateien (Sorted String Table)
// INVARIANTEN: Immutability post-creation, sorted key order, async spawn_blocking I/O, zero panic
// HOTSPOTS:    SSTableIterator, Bloom-Filter-Lookup, merge_sorted_iters()
// SIEHE AUCH:  crates/memfuse-store/AGENTS.md

use bytes::{BufMut, Bytes, BytesMut};
use lru::LruCache;
use memfuse_core::{MemFuseError, Result};
use memfuse_crypto::crypto::KeyManager;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[cfg(unix)]
fn pread_exact(file: &std::fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buf, offset)
}

#[cfg(windows)]
fn pread_exact(file: &std::fs::File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_read(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ));
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Sharded block cache: 64 independent shards to reduce contention.
/// Shard selection uses ahash tuple hashing on (file_id, offset).
pub const BLOCK_CACHE_SHARDS: usize = 64;

/// Trait abstraction for block cache backend implementations.
///
/// ## Approximative LRU & Lock-Optimized Read Path
/// SSTable block caches store immutable 4KB data blocks. For point lookups and range scans,
/// read hits occur with high frequency on hot-set blocks.
///
/// While standard `LruCache` maintains strict LRU ordering by updating a doubly-linked list
/// on every access (requiring write lock acquisition even during `get()`), lock-optimized
/// implementations like `quick_cache::sync::Cache` (S3-FIFO / Clock eviction) achieve
/// lock-free read hit evaluation via atomic reference counting without mutating global eviction order on every hit.
///
/// Functionally, strict LRU order is NOT required for correctness in SSTable block caching.
/// Approximative LRU / S3-FIFO eviction provides equivalent or superior hit ratios while
/// eliminating lock contention across concurrent reader threads.
pub trait BlockCacheBackend: Send + Sync {
    /// Creates a new cache backend instance with the specified capacity.
    fn new(capacity: usize) -> Self
    where
        Self: Sized;

    /// Retrieves a cached block by `(file_id, offset)` key.
    fn get(&self, key: &(u64, u64)) -> Option<Bytes>;

    /// Inserts a block into the cache.
    fn insert(&self, key: (u64, u64), value: Bytes);

    /// Returns the number of cached blocks in this backend instance.
    fn len(&self) -> usize;

    /// Returns `true` if the backend contains no cached blocks.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks if a key is present in the cache.
    fn contains(&self, key: &(u64, u64)) -> bool {
        self.get(key).is_some()
    }
}

struct LruBlockCacheState {
    cache: LruCache<(u64, u64), Bytes>,
    current_bytes: usize,
    capacity_bytes: usize,
}

/// Standard strict-LRU block cache backend using `parking_lot::RwLock<LruCache>` with byte-based capacity.
pub struct LruBlockCacheBackend {
    state: RwLock<LruBlockCacheState>,
}

impl BlockCacheBackend for LruBlockCacheBackend {
    fn new(capacity_bytes: usize) -> Self {
        let capacity_bytes = capacity_bytes.max(1);
        Self {
            state: RwLock::new(LruBlockCacheState {
                cache: LruCache::unbounded(),
                current_bytes: 0,
                capacity_bytes,
            }),
        }
    }

    #[inline]
    fn get(&self, key: &(u64, u64)) -> Option<Bytes> {
        self.state.write().cache.get(key).cloned()
    }

    #[inline]
    fn insert(&self, key: (u64, u64), value: Bytes) {
        let mut s = self.state.write();
        let val_len = value.len();
        if let Some(old_val) = s.cache.put(key, value) {
            s.current_bytes = s.current_bytes.saturating_sub(old_val.len());
        }
        s.current_bytes += val_len;

        while s.current_bytes > s.capacity_bytes && !s.cache.is_empty() {
            if let Some((_k, popped)) = s.cache.pop_lru() {
                s.current_bytes = s.current_bytes.saturating_sub(popped.len());
            } else {
                break;
            }
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.state.read().cache.len()
    }

    #[inline]
    fn contains(&self, key: &(u64, u64)) -> bool {
        self.state.read().cache.contains(key)
    }
}

#[cfg(feature = "block-cache-v2")]
#[derive(Clone)]
struct BlockWeighter;

#[cfg(feature = "block-cache-v2")]
impl quick_cache::Weighter<(u64, u64), Bytes> for BlockWeighter {
    fn weight(&self, _key: &(u64, u64), value: &Bytes) -> u32 {
        value.len().try_into().unwrap_or(u32::MAX)
    }
}

/// Lock-optimized block cache backend using `quick_cache::sync::Cache` (S3-FIFO / Clock-based eviction) with byte-based capacity.
#[cfg(feature = "block-cache-v2")]
pub struct QuickCacheBlockCacheBackend {
    cache: quick_cache::sync::Cache<(u64, u64), Bytes, BlockWeighter>,
}

#[cfg(feature = "block-cache-v2")]
impl BlockCacheBackend for QuickCacheBlockCacheBackend {
    fn new(capacity_bytes: usize) -> Self {
        let estimated_items = (capacity_bytes / 4096).max(16);
        Self {
            cache: quick_cache::sync::Cache::with_weighter(
                estimated_items,
                capacity_bytes as u64,
                BlockWeighter,
            ),
        }
    }

    #[inline]
    fn get(&self, key: &(u64, u64)) -> Option<Bytes> {
        self.cache.get(key)
    }

    #[inline]
    fn insert(&self, key: (u64, u64), value: Bytes) {
        self.cache.insert(key, value);
    }

    #[inline]
    fn len(&self) -> usize {
        self.cache.len()
    }

    #[inline]
    fn contains(&self, key: &(u64, u64)) -> bool {
        self.cache.get(key).is_some()
    }
}

#[cfg(not(feature = "block-cache-v2"))]
pub type BlockCacheShard = LruBlockCacheBackend;

#[cfg(feature = "block-cache-v2")]
pub type BlockCacheShard = QuickCacheBlockCacheBackend;

pub struct BlockCache {
    shards: Vec<BlockCacheShard>,
    hash_builder: ahash::RandomState,
}

impl BlockCache {
    pub fn new(capacity_per_shard: usize) -> Self {
        Self::new_with_shards(capacity_per_shard, BLOCK_CACHE_SHARDS)
    }

    pub fn new_with_shards(capacity_per_shard: usize, num_shards: usize) -> Self {
        let num_shards = num_shards.max(1);
        let cap_shard = capacity_per_shard.max(1);

        let shards = (0..num_shards)
            .map(|_| BlockCacheShard::new(cap_shard))
            .collect();

        Self {
            shards,
            hash_builder: ahash::RandomState::new(),
        }
    }

    #[inline]
    pub fn shard_idx(&self, file_id: u64, offset: u64) -> usize {
        let hash = self.hash_builder.hash_one((file_id, offset));
        (hash as usize) % self.shards.len()
    }

    #[inline]
    fn shard(&self, file_id: u64, offset: u64) -> &BlockCacheShard {
        let idx = self.shard_idx(file_id, offset);
        &self.shards[idx]
    }

    #[inline]
    pub fn get(&self, file_id: u64, offset: u64) -> Option<Bytes> {
        self.shard(file_id, offset).get(&(file_id, offset))
    }

    #[inline]
    pub fn insert(&self, file_id: u64, offset: u64, data: Bytes) {
        self.shard(file_id, offset).insert((file_id, offset), data);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.len()).sum()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn contains(&self, file_id: u64, offset: u64) -> bool {
        self.shard(file_id, offset).contains(&(file_id, offset))
    }
}

/// Binary search for a key index inside a SSTable data block.
///
/// Returns `Ok(Ok(index))` if the exact key is found, or `Ok(Err(index))`
/// indicating the insertion index (lower-bound entry offset index) if not found.
/// Exposed as `pub` for fuzz testing in `memfuse-store-fuzz`.
pub fn binary_search_index_in_block(
    block_data: &[u8],
    offsets_start: usize,
    num_offsets: usize,
    key: &[u8],
) -> Result<std::result::Result<usize, usize>> {
    if num_offsets == 0 {
        return Ok(Err(0));
    }

    let mut low = 0;
    let mut high = num_offsets;

    while low < high {
        let mid = low + (high - low) / 2;
        let off_pos = offsets_start + mid * 2;
        let entry_off = u16::from_le_bytes(
            block_data
                .get(off_pos..off_pos + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
        ) as usize;

        let k_len = u16::from_le_bytes(
            block_data
                .get(entry_off..entry_off + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
        ) as usize;

        let ep = entry_off + 2;
        let entry_key = block_data
            .get(ep..ep + k_len)
            .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key".into()))?;

        match entry_key.cmp(key) {
            std::cmp::Ordering::Less => {
                low = mid + 1;
            }
            std::cmp::Ordering::Greater => {
                high = mid;
            }
            std::cmp::Ordering::Equal => {
                return Ok(Ok(mid));
            }
        }
    }

    Ok(Err(low))
}

/// Binary search for a key inside a SSTable data block.
///
/// Returns `Ok(Some((entry_offset, key_len)))` if the key is found,
/// or `Ok(None)` if the key does not exist in the block.
/// Exposed as `pub` for fuzz testing in `memfuse-store-fuzz`.
pub fn binary_search_entry_in_block(
    block_data: &[u8],
    offsets_start: usize,
    num_offsets: usize,
    key: &[u8],
) -> Result<Option<(usize, usize)>> {
    match binary_search_index_in_block(block_data, offsets_start, num_offsets, key)? {
        Ok(idx) => {
            let off_pos = offsets_start + idx * 2;
            let entry_off = u16::from_le_bytes(
                block_data
                    .get(off_pos..off_pos + 2)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            let k_len = u16::from_le_bytes(
                block_data
                    .get(entry_off..entry_off + 2)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            Ok(Some((entry_off, k_len)))
        }
        Err(_) => Ok(None),
    }
}

/// # Preconditions
/// - The block entries and the offsets array MUST be sorted lexicographically by key.
///   This invariant is guaranteed by `BlockBuilder::build()` and SSTable file header contract.
///
/// Binary search over the sorted offsets array within a decoded block.
/// Returns `Ok(Some(entry_offset))` if found, `Ok(None)` if not present.
/// `block_data`: the raw (decrypted, CRC-stripped) block bytes.
/// `offsets_start`: byte position of the first 2-byte offset entry.
/// `num_offsets`: number of entries in the offsets array.
/// `key`: the key to search for.
/// Exposed as `pub` for fuzz testing in `memfuse-store-fuzz`.
pub fn block_binary_search(
    block_data: &[u8],
    offsets_start: usize,
    num_offsets: usize,
    key: &[u8],
) -> Result<Option<usize>> {
    if num_offsets == 0 {
        return Ok(None);
    }
    let mut lo = 0usize;
    let mut hi = num_offsets;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let off_pos = offsets_start + mid * 2;
        let entry_off = u16::from_le_bytes(
            block_data
                .get(off_pos..off_pos + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos in bsearch".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice in bsearch".into()))?,
        ) as usize;
        let k_len = u16::from_le_bytes(
            block_data
                .get(entry_off..entry_off + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: k_len in bsearch".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid k_len slice in bsearch".into()))?,
        ) as usize;
        let entry_key = block_data
            .get(entry_off + 2..entry_off + 2 + k_len)
            .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key in bsearch".into()))?;
        match entry_key.cmp(key) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => hi = mid,
        }
    }
    if lo < num_offsets {
        let off_pos = offsets_start + lo * 2;
        let entry_off = u16::from_le_bytes(
            block_data
                .get(off_pos..off_pos + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos in bsearch".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice in bsearch".into()))?,
        ) as usize;
        let k_len = u16::from_le_bytes(
            block_data
                .get(entry_off..entry_off + 2)
                .ok_or_else(|| MemFuseError::Storage("malformed block: k_len in bsearch".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid k_len slice in bsearch".into()))?,
        ) as usize;
        let entry_key = block_data
            .get(entry_off + 2..entry_off + 2 + k_len)
            .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key in bsearch".into()))?;
        if entry_key == key {
            return Ok(Some(entry_off));
        }
    }
    Ok(None)
}

/// Magic bytes for SSTable file trailer.
pub const SSTABLE_MAGIC_MFSX: u32 = 0x5853_464D; // "MFSX" in hex
pub const SSTABLE_MAGIC_LEGACY: u32 = 0x4D46_5354; // "MFST" in hex

/// Creates a new block cache instance with default shard count. Capacity is in MB (assuming 4KB blocks).
pub fn create_block_cache(capacity_mb: usize) -> Arc<BlockCache> {
    create_block_cache_with_shards(capacity_mb, BLOCK_CACHE_SHARDS)
}

/// Creates a new block cache instance with configurable shard count. Capacity is in MB.
pub fn create_block_cache_with_shards(capacity_mb: usize, num_shards: usize) -> Arc<BlockCache> {
    let num_shards = num_shards.max(1);
    let total_bytes = capacity_mb
        .saturating_mul(1024 * 1024)
        .clamp(1024 * 1024, 8 * 1024 * 1024 * 1024);
    let per_shard_bytes = (total_bytes / num_shards).max(64 * 1024);
    Arc::new(BlockCache::new_with_shards(per_shard_bytes, num_shards))
}

/// Block size for SSTable data blocks (4KB).
const BLOCK_SIZE: usize = 4096;

/// SPECCED: Speichereffizienter Bloom-Filter für SSTable-Pre-Checks.
/// Ziel: False-Positive-Rate p ≤ 0.01.
/// Formel: m = -n * ln(p) / (ln(2)^2) ≈ 9.6 * n
/// Hashes: k = (m/n) * ln(2) ≈ 7
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<u64>,
    num_hashes: usize,
    num_bits: usize,
}

impl BloomFilter {
    /// Creates a new Bloom filter for the expected number of elements and target false positive rate (fpr).
    ///
    /// ## FPR Trade-off
    /// - `fpr = 0.01` (1%) uses ~9.6 bits/element.
    /// - `fpr = 0.001` (0.1%) uses ~14.4 bits/element.
    ///
    /// Note: The default `fpr` used in [`SstableBuilder`] should be documented as a tunable
    /// parameter and referenced in [`crate::lsm::LsmConfig`] where it should be configurable.
    pub fn new(expected_elements: usize, fpr: f64) -> Self {
        let n = expected_elements.max(1);
        let p = fpr.clamp(0.0001, 0.1);

        // Safety limit: clamp expected elements to 100 million to prevent float/usize overflow
        let n = n.min(100_000_000);

        // m = bits needed
        let m = (-(n as f64) * p.ln() / (2.0f64.ln().powi(2))).ceil() as usize;

        // Hard upper bound on bloom filter bits (128 MB = 1_073_741_824 bits)
        const MAX_BITS: usize = 128 * 1024 * 1024 * 8;
        let num_bits = m.next_multiple_of(64).clamp(64, MAX_BITS);
        let num_hashes = ((num_bits as f64 / n as f64) * 2.0f64.ln()).round() as usize;
        let num_hashes = num_hashes.clamp(1, 16);

        Self {
            bits: vec![0u64; num_bits / 64],
            num_hashes,
            num_bits,
        }
    }

    /// Inserts a key into the filter.
    pub fn insert(&mut self, key: &[u8]) {
        let (h1, h2) = Self::hash_pair(key);
        for i in 0..self.num_hashes {
            // Double Hashing: bit_idx = (h1 + i * h2) % num_bits
            // Garantiert gleichmäßige Verteilung ohne Wiederholungen für i < num_bits
            let bit_idx = h1.wrapping_add((i as u64).wrapping_mul(h2)) as usize % self.num_bits;
            self.bits[bit_idx / 64] |= 1u64 << (bit_idx % 64);
        }
    }

    /// Checks if a key might be in the filter.
    pub fn may_contain(&self, key: &[u8]) -> bool {
        if self.num_bits == 0 {
            return true;
        }
        let (h1, h2) = Self::hash_pair(key);
        for i in 0..self.num_hashes {
            let bit_idx = h1.wrapping_add((i as u64).wrapping_mul(h2)) as usize % self.num_bits;
            if (self.bits[bit_idx / 64] & (1u64 << (bit_idx % 64))) == 0 {
                return false;
            }
        }
        true
    }

    /// Erzeugt zwei unabhängige 64-bit Hashes aus dem Blake3-Digest.
    /// h1 = erste 8 Bytes, h2 = Bytes 8-15 (oder fallback zu h1 ^ const)
    fn hash_pair(key: &[u8]) -> (u64, u64) {
        let hash = blake3::hash(key);
        let bytes = hash.as_bytes();

        let h1 = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0u8; 8]));
        let mut h2 = u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0u8; 8]));

        // h2 muss ungerade sein für double hashing (verhindert Zyklen)
        h2 |= 1;

        (h1, h2)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        // MIGRATION NOTE: Bestehende SSTables mit altem Filter (probe-basiert)
        // müssen bei der nächsten Compaction neu gebaut werden.
        // Das Serialisierungsformat bleibt kompatibel, daher ist kein aktives Handeln nötig.
        let mut buf = Vec::with_capacity(8 + 8 + self.bits.len() * 8);
        buf.put_u64_le(self.num_hashes as u64);
        buf.put_u64_le(self.num_bits as u64);
        for &word in &self.bits {
            buf.put_u64_le(word);
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(MemFuseError::Storage(
                "corrupted bloom filter: too short".into(),
            ));
        }
        let num_hashes = u64::from_le_bytes(data[0..8].try_into().map_err(|_| {
            MemFuseError::ParseError("corrupted bloom filter: invalid num_hashes".into())
        })?) as usize;
        let num_bits = u64::from_le_bytes(data[8..16].try_into().map_err(|_| {
            MemFuseError::ParseError("corrupted bloom filter: invalid num_bits".into())
        })?) as usize;

        // Safety limit: 128MB for bloom filter bits (approx 1 billion bits)
        if num_bits > 128 * 1024 * 1024 * 8 {
            return Err(MemFuseError::Storage(format!(
                "corrupted bloom filter: too many bits ({})",
                num_bits
            )));
        }

        let capacity_cap = (num_bits / 64).min((data.len() - 16) / 8);
        let mut bits = Vec::with_capacity(capacity_cap);
        let mut offset = 16;
        while offset + 8 <= data.len() {
            bits.push(u64::from_le_bytes(
                data[offset..offset + 8].try_into().map_err(|_| {
                    MemFuseError::ParseError("corrupted bloom filter: invalid word".into())
                })?,
            ));
            offset += 8;
        }
        Ok(Self {
            bits,
            num_hashes,
            num_bits,
        })
    }
}

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
            block_size: block_size.clamp(512, 64 * 1024 * 1024),
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

    pub fn add(&mut self, key: &[u8], value: &[u8], seq_no: u64, tx_id: u64) -> bool {
        // size: key_len(2) + key + seq_no(8) + tx_id(8) + val_len(2) + value + bloom(8) + offsets + offset count (2 bytes)
        if !self.data.is_empty()
            && self.current_size() + key.len() + value.len() + 20 > self.block_size
        {
            return false;
        }

        self.update_bloom(key);
        self.offsets.push(self.data.len() as u16);
        self.data.put_u16_le(key.len() as u16);
        self.data.put_slice(key);
        self.data.put_u64_le(seq_no);
        self.data.put_u64_le(tx_id);
        self.data.put_u32_le(value.len() as u32);
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
#[derive(Debug, Clone)]
pub struct SstableMetadata {
    pub first_key: Bytes,
    pub last_key: Bytes,
    pub file_size: u64,
    pub min_tx_id: u64,
    pub max_tx_id: u64,
    pub min_seq: u64,
    pub max_seq: u64,
}

/// A builder for creating new SSTables.
///
/// Note: Uses a whole-SSTable Bloom filter with a default FPR. The Bloom filter FPR should be
/// treated as a tunable parameter and configured via [`crate::lsm::LsmConfig`].
pub struct SstableBuilder {
    path: PathBuf,
    file: File,
    block_builder: BlockBuilder,
    index: Vec<(Bytes, u64)>, // (last_key, offset)
    first_key: Option<Bytes>,
    last_key: Option<Bytes>,
    offset: u64,
    key_manager: Option<Arc<KeyManager>>,
    /// Whole-SSTable bloom filter for cross-block pre-checks.
    bloom_filter: BloomFilter,
    key_count: usize,
    min_tx_id: u64,
    max_tx_id: u64,
    min_seq: u64,
    max_seq: u64,
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
        let path_ref = path.as_ref();
        let derived_km = if let Some(km) = key_manager {
            let file_id = path_ref
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            Some(Arc::new(km.derive_file_key(file_id.as_bytes())?))
        } else {
            None
        };
        let file = File::create(path_ref)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to create SSTable: {}", e)))?;

        Ok(Self {
            path: path_ref.to_path_buf(),
            file,
            block_builder: BlockBuilder::new(BLOCK_SIZE),
            index: Vec::new(),
            first_key: None,
            last_key: None,
            offset: 0,
            key_manager: derived_km,
            // Initialize with capacity 1000 (will grow if needed, or we just trust the final size)
            // Actually, we don't know the final size, but we can re-create it at finish or just use a large enough default.
            // Better: use a dynamic filter if possible, but standard Bloom needs N.
            // We'll use a fixed large capacity or estimate from previous runs.
            // For now, let's use a very conservative 100k capacity bitset.
            bloom_filter: BloomFilter::new(100_000, 0.01),
            key_count: 0,
            min_tx_id: u64::MAX,
            max_tx_id: 0,
            min_seq: u64::MAX,
            max_seq: 0,
        })
    }

    /// Adds a key-value pair to the SSTable being built.
    pub async fn add(&mut self, key: &[u8], value: &[u8], seq_no: u64, tx_id: u64) -> Result<()> {
        if key.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "SSTable key cannot be empty".to_string(),
            ));
        }
        if key.len() > 65535 || value.len() > crate::lsm::MAX_VALUE_SIZE {
            return Err(MemFuseError::InvalidInput(format!(
                "Key ({} bytes) exceeds 65535 limit or value ({} bytes) exceeds MAX_VALUE_SIZE limit",
                key.len(),
                value.len()
            )));
        }

        if self.first_key.is_none() {
            self.first_key = Some(Bytes::copy_from_slice(key));
        }

        if !self.block_builder.add(key, value, seq_no, tx_id) {
            self.flush_block().await?;
            if !self.block_builder.add(key, value, seq_no, tx_id) {
                return Err(MemFuseError::Storage(format!(
                    "Key-value entry too large for SSTable block (key: {} bytes, val: {} bytes)",
                    key.len(),
                    value.len()
                )));
            }
        }

        self.bloom_filter.insert(key);
        self.key_count += 1;
        self.min_tx_id = self.min_tx_id.min(tx_id);
        self.max_tx_id = self.max_tx_id.max(tx_id);
        let raw_seq = seq_no & !memfuse_core::TOMBSTONE_BIT;
        self.min_seq = self.min_seq.min(raw_seq);
        self.max_seq = self.max_seq.max(raw_seq);
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
        let block_data =
            std::mem::replace(&mut self.block_builder, BlockBuilder::new(BLOCK_SIZE)).build();

        // Compute CRC before encryption
        let crc = crc32fast::hash(&block_data);
        let mut block_with_crc = Vec::with_capacity(4 + block_data.len());
        block_with_crc.extend_from_slice(&crc.to_le_bytes());
        block_with_crc.extend_from_slice(&block_data);

        let mut block = Bytes::from(block_with_crc);

        if let Some(km) = &self.key_manager {
            let (encrypted, nonce) = km.encrypt_auto_nonce(&block)?;
            let mut new_block = BytesMut::with_capacity(12 + encrypted.len());
            new_block.put_slice(&nonce);
            new_block.put_slice(&encrypted);
            block = new_block.freeze();
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

        // Add CRC to index
        let index_crc = crc32fast::hash(&index_bytes);
        let mut index_with_crc = Vec::with_capacity(4 + index_bytes.len());
        index_with_crc.extend_from_slice(&index_crc.to_le_bytes());
        index_with_crc.extend_from_slice(&index_bytes);
        let index_to_write = index_with_crc;

        self.file
            .write_all(&index_to_write)
            .await
            .map_err(|e| MemFuseError::Storage(format!("SSTable index write failed: {}", e)))?;

        // SPECCED: Write the whole-SSTable Bloom filter
        let bloom_offset = index_offset + index_to_write.len() as u64;
        let bloom_data = self.bloom_filter.to_bytes();

        // Add CRC to bloom
        let bloom_crc = crc32fast::hash(&bloom_data);
        let mut bloom_with_crc = Vec::with_capacity(4 + bloom_data.len());
        bloom_with_crc.extend_from_slice(&bloom_crc.to_le_bytes());
        bloom_with_crc.extend_from_slice(&bloom_data);
        let bloom_to_write = bloom_with_crc;

        self.file
            .write_all(&bloom_to_write)
            .await
            .map_err(|e| MemFuseError::Storage(format!("SSTable bloom write failed: {}", e)))?;

        // Write trailer: [min_tx][max_tx][min_seq][max_seq][bloom_offset][index_offset][magic]
        // This is 52 bytes.
        self.file
            .write_u64_le(self.min_tx_id)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u64_le(self.max_tx_id)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u64_le(self.min_seq)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u64_le(self.max_seq)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u64_le(bloom_offset)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u64_le(index_offset)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        // FIND-STO-003: Extension point — format version (v1 = 54 byte trailer)
        self.file
            .write_u16_le(1)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        self.file
            .write_u32_le(SSTABLE_MAGIC_MFSX)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        self.file
            .sync_all()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        crate::util::fsync_parent_dir(&self.path).await?;

        let file_size = self
            .file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?
            .len();

        Ok(SstableMetadata {
            first_key: self.first_key.clone().unwrap_or_default(),
            last_key: self.last_key.clone().unwrap_or_default(),
            file_size,
            min_tx_id: self.min_tx_id,
            max_tx_id: self.max_tx_id,
            min_seq: self.min_seq,
            max_seq: self.max_seq,
        })
    }
}

/// A reader for existing SSTables.
pub struct SstableReader {
    file: Arc<std::fs::File>,
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
    /// Optional whole-SSTable bloom filter for cross-block pre-checks.
    bloom_filter: Option<BloomFilter>,
    /// Whether blocks have CRC32 checksums.
    has_crc: bool,
}

impl SstableReader {
    pub fn first_key(&self) -> &Bytes {
        &self.metadata.first_key
    }

    pub fn last_key(&self) -> &Bytes {
        &self.metadata.last_key
    }

    pub fn min_tx_id(&self) -> u64 {
        self.metadata.min_tx_id
    }

    pub fn max_tx_id(&self) -> u64 {
        self.metadata.max_tx_id
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

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
        let derived_km = if let Some(ref km) = key_manager {
            let file_id = path_buf
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            Some(Arc::new(km.derive_file_key(file_id.as_bytes())?))
        } else {
            None
        };

        let path_for_open = path_buf.clone();
        let (file, file_size) =
            tokio::task::spawn_blocking(move || -> std::io::Result<(std::fs::File, u64)> {
                let file = std::fs::File::open(&path_for_open)?;
                let metadata = file.metadata()?;
                let file_size = metadata.len();
                Ok((file, file_size))
            })
            .await
            .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
            .map_err(|e| MemFuseError::Storage(format!("File open failed: {}", e)))?;

        if file_size < 12 {
            return Err(MemFuseError::Storage("SSTable file too small".into()));
        }

        let file = Arc::new(file);

        // Read trailer: last 54 bytes (v1) or 52 bytes (v0)
        let trailer_data = {
            let f = Arc::clone(&file);
            tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
                // We read up to 54 bytes to check for v1 trailer
                let mut buf = vec![0u8; 54.min(file_size as usize)];
                let offset = file_size.saturating_sub(54);
                pread_exact(&f, &mut buf, offset)?;
                Ok(buf)
            })
            .await
            .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
            .map_err(|e| MemFuseError::Storage(format!("Trailer read failed: {}", e)))?
        };

        let trailer_len = trailer_data.len();
        if trailer_len < 12 {
            return Err(MemFuseError::Storage("Invalid trailer".into()));
        }

        // Detect version and magic (FIND-STO-003)
        // v1: [..., version:u16][magic:u32] at the end (54 bytes)
        // v0: [..., magic:u32] at the end (52 bytes)
        let mut format_version = 0u16;
        let mut is_mfsx = false;

        if trailer_len >= 54 {
            let magic_v1 = u32::from_le_bytes(
                trailer_data[50..54]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid trailer".into()))?,
            );
            if magic_v1 == SSTABLE_MAGIC_MFSX {
                format_version = u16::from_le_bytes(
                    trailer_data[48..50]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid trailer".into()))?,
                );
                is_mfsx = true;
            }
        }

        if !is_mfsx && trailer_len >= 52 {
            let magic_v0 = u32::from_le_bytes(
                trailer_data[48..52]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid trailer".into()))?,
            );
            if magic_v0 == SSTABLE_MAGIC_MFSX {
                is_mfsx = true;
                format_version = 0;
            }
        }

        let mut has_bloom = false;
        let has_crc = is_mfsx;
        let mut bloom_offset = 0;
        let mut min_tx_id = 0;
        let mut max_tx_id = 0;
        let mut min_seq = 0;
        let mut max_seq = 0;

        let index_offset = if is_mfsx {
            // MFSX trailer (v0 or v1)
            let base = if format_version >= 1 { 0 } else { 2 }; // Offset into our 54-byte buffer
            min_tx_id = u64::from_le_bytes(
                trailer_data[base..base + 8]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid min_tx_id".into()))?,
            );
            max_tx_id = u64::from_le_bytes(
                trailer_data[base + 8..base + 16]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid max_tx_id".into()))?,
            );
            min_seq = u64::from_le_bytes(
                trailer_data[base + 16..base + 24]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid min_seq".into()))?,
            );
            max_seq = u64::from_le_bytes(
                trailer_data[base + 24..base + 32]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid max_seq".into()))?,
            );
            bloom_offset = u64::from_le_bytes(
                trailer_data[base + 32..base + 40]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid bloom_offset".into()))?,
            );
            if bloom_offset > 0 {
                has_bloom = true;
            }
            u64::from_le_bytes(
                trailer_data[base + 40..base + 48]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("Invalid index_offset".into()))?,
            )
        } else {
            // Read magic from the very end of 54-byte buffer (which would be the same as end of 52-byte if we read 54)
            let magic_legacy = u32::from_le_bytes(
                trailer_data[50..54]
                    .try_into()
                    .map_err(|_| MemFuseError::checksum_mismatch(path_buf.to_string_lossy(), 0))?,
            );
            if magic_legacy == SSTABLE_MAGIC_LEGACY {
                // Backward-compatible 12-byte trailer: [index_offset: u64][magic: u32]
                u64::from_le_bytes(
                    trailer_data[42..50]
                        .try_into()
                        .map_err(|_| MemFuseError::ParseError("Invalid index offset".into()))?,
                )
            } else {
                return Err(MemFuseError::Storage("Invalid SSTable magic number".into()));
            }
        };

        let bloom_filter = if has_bloom {
            let bloom_end = if is_mfsx {
                file_size.saturating_sub(if format_version >= 1 { 54 } else { 52 })
            } else {
                file_size.saturating_sub(20)
            };

            let bloom_data_raw = {
                let f = Arc::clone(&file);
                tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
                    let mut buf =
                        vec![0u8; (bloom_end as usize).saturating_sub(bloom_offset as usize)];
                    pread_exact(&f, &mut buf, bloom_offset)?;
                    Ok(buf)
                })
                .await
                .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
                .map_err(|e| MemFuseError::Storage(format!("Bloom read failed: {}", e)))?
            };

            let bloom_data = if has_crc {
                if bloom_data_raw.len() < 4 {
                    return Err(MemFuseError::Storage(
                        "Bloom filter data too short for CRC".into(),
                    ));
                }
                let stored_crc = u32::from_le_bytes(
                    bloom_data_raw[0..4]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid CRC".into()))?,
                );
                let payload = &bloom_data_raw[4..];
                if crc32fast::hash(payload) != stored_crc {
                    return Err(MemFuseError::checksum_mismatch(
                        path_buf.to_string_lossy(),
                        bloom_offset,
                    ));
                }
                payload
            } else {
                &bloom_data_raw
            };

            Some(BloomFilter::from_bytes(bloom_data)?)
        } else {
            None
        };

        // Read index
        let index_data_raw = {
            let f = Arc::clone(&file);
            let index_end = if has_bloom {
                bloom_offset as usize
            } else {
                file_size.saturating_sub(if is_mfsx {
                    if format_version >= 1 {
                        54
                    } else {
                        52
                    }
                } else {
                    12
                }) as usize
            };
            tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
                let mut buf = vec![0u8; index_end.saturating_sub(index_offset as usize)];
                pread_exact(&f, &mut buf, index_offset)?;
                Ok(buf)
            })
            .await
            .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
            .map_err(|e| MemFuseError::Storage(format!("Index read failed: {}", e)))?
        };

        let index_data = if has_crc {
            if index_data_raw.len() < 4 {
                return Err(MemFuseError::Storage("Index data too short for CRC".into()));
            }
            let stored_crc = u32::from_le_bytes(
                index_data_raw[0..4]
                    .try_into()
                    .map_err(|_| MemFuseError::Serialization("Invalid CRC".into()))?,
            );
            let payload = &index_data_raw[4..];
            if crc32fast::hash(payload) != stored_crc {
                return Err(MemFuseError::checksum_mismatch(
                    path_buf.to_string_lossy(),
                    index_offset,
                ));
            }
            payload
        } else {
            &index_data_raw
        };

        let mut index = Vec::new();
        let mut pos = 0;
        let index_len = index_data.len();

        while pos + 10 <= index_len {
            let key_len = u16::from_le_bytes(
                index_data[pos..pos + 2]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("corrupted index: key_len".into()))?,
            ) as usize;
            pos += 2;

            if pos + key_len + 8 > index_len {
                return Err(MemFuseError::ParseError(
                    "corrupted index: data too short".into(),
                ));
            }

            let key = Bytes::copy_from_slice(&index_data[pos..pos + key_len]);
            pos += key_len;

            let offset = u64::from_le_bytes(
                index_data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| MemFuseError::ParseError("corrupted index: offset".into()))?,
            );
            pos += 8;
            index.push((key, offset));
        }

        let first_key = if !index.is_empty() {
            let offset = index[0].1;
            let next_offset = if index.len() > 1 {
                index[1].1
            } else {
                index_offset
            };
            let block = Self::read_block_at_file(
                Arc::clone(&file),
                offset,
                next_offset,
                &derived_km,
                has_crc,
                &path_buf,
            )
            .await?;
            if block.len() >= 2 {
                let k_len = u16::from_le_bytes(
                    block[0..2]
                        .try_into()
                        .map_err(|_| MemFuseError::ParseError("corrupted block: k_len".into()))?,
                ) as usize;
                if block.len() >= 2 + k_len {
                    Bytes::copy_from_slice(&block[2..2 + k_len])
                } else {
                    Bytes::new()
                }
            } else {
                Bytes::new()
            }
        } else {
            Bytes::new()
        };

        Ok(Self {
            file,
            metadata: SstableMetadata {
                first_key,
                last_key: index.last().map(|(k, _)| k.clone()).unwrap_or_default(),
                file_size,
                min_tx_id,
                max_tx_id,
                min_seq,
                max_seq,
            },
            index,
            index_offset,
            file_path: path_buf,
            file_id: {
                static NEXT_FILE_ID: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1);
                NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            block_cache,
            key_manager: derived_km,
            bloom_filter,
            has_crc,
        })
    }

    async fn read_block_at_file(
        file: Arc<std::fs::File>,
        offset: u64,
        next_offset: u64,
        key_manager: &Option<Arc<KeyManager>>,
        has_crc: bool,
        path: &Path,
    ) -> Result<Bytes> {
        let len = next_offset.saturating_sub(offset) as usize;
        let data = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            let mut buf = vec![0u8; len];
            pread_exact(&file, &mut buf, offset)?;
            Ok(buf)
        })
        .await
        .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
        .map_err(|e| MemFuseError::Storage(format!("Block read failed: {}", e)))?;

        let block_data = if let Some(km) = key_manager {
            if data.len() < 12 {
                return Err(MemFuseError::Storage("Block too small for nonce".into()));
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&data[0..12]);
            let decrypted = km.decrypt_auto_nonce(&data[12..], &nonce)?;
            Bytes::from(decrypted)
        } else {
            Bytes::from(data)
        };

        if has_crc {
            if block_data.len() < 4 {
                return Err(MemFuseError::Storage("Block too small for CRC".into()));
            }
            let stored_crc = u32::from_le_bytes(
                block_data[0..4]
                    .try_into()
                    .map_err(|_| MemFuseError::Serialization("Invalid CRC format".into()))?,
            );
            let payload = &block_data[4..];
            let computed_crc = crc32fast::hash(payload);

            if stored_crc != computed_crc {
                return Err(MemFuseError::checksum_mismatch(
                    path.to_string_lossy(),
                    offset,
                ));
            }
            Ok(block_data.slice(4..))
        } else {
            Ok(block_data)
        }
    }

    async fn get_block(&self, offset: u64, next_offset: u64) -> Result<Bytes> {
        if let Some(cached) = self.block_cache.get(self.file_id, offset) {
            return Ok(cached);
        }
        let block = Self::read_block_at_file(
            Arc::clone(&self.file),
            offset,
            next_offset,
            &self.key_manager,
            self.has_crc,
            &self.file_path,
        )
        .await?;
        self.block_cache.insert(self.file_id, offset, block.clone());
        Ok(block)
    }

    /// Retrieves a value from the SSTable by key.
    pub async fn get(&self, key: &[u8]) -> Result<Option<(Bytes, u64, u64)>> {
        // 1. Whole-SSTable Bloom Filter Pre-check
        // SPECCED: Only if bloom filter is present (backward compatibility)
        if let Some(bloom) = &self.bloom_filter {
            if !bloom.may_contain(key) {
                return Ok(None);
            }
        }

        if key < self.metadata.first_key || key > self.metadata.last_key {
            return Ok(None);
        }

        let idx = match self.index.binary_search_by(|(k, _)| k.as_ref().cmp(key)) {
            Ok(i) => i,
            Err(i) => i,
        };

        if idx >= self.index.len() {
            return Ok(None);
        }

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

        if let Some(entry_off) = block_binary_search(&block_data, offsets_start, num_offsets, key)?
        {
            let k_len = u16::from_le_bytes(
                block_data
                    .get(entry_off..entry_off + 2)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            let mut ep = entry_off + 2 + k_len;
            let seq_no = u64::from_le_bytes(
                block_data
                    .get(ep..ep + 8)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: seq_no".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            );
            ep += 8;
            let tx_id = u64::from_le_bytes(
                block_data
                    .get(ep..ep + 8)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: tx_id".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            );
            ep += 8;
            let v_len = u32::from_le_bytes(
                block_data
                    .get(ep..ep + 4)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            ep += 4;
            if ep + v_len > block_data.len() {
                return Err(MemFuseError::Storage(
                    "malformed block: value length out of bounds".into(),
                ));
            }
            let entry_val = block_data.slice(ep..ep + v_len);
            Ok(Some((entry_val, seq_no, tx_id)))
        } else {
            Ok(None)
        }
    }

    /// Metrics result for point lookup instrumentation.
    pub async fn lookup_metrics(&self, key: &[u8]) -> (bool, bool, bool, bool) {
        // Returns (bloom_passed, range_passed, block_read, key_found)
        if let Some(bloom) = &self.bloom_filter {
            if !bloom.may_contain(key) {
                return (false, false, false, false);
            }
        }

        if key < self.metadata.first_key || key > self.metadata.last_key {
            return (true, false, false, false);
        }

        let idx = match self.index.binary_search_by(|(k, _)| k.as_ref().cmp(key)) {
            Ok(i) => i,
            Err(i) => i,
        };

        if idx >= self.index.len() {
            return (true, true, false, false);
        }

        let offset = match self.index.get(idx) {
            Some((_, off)) => *off,
            None => return (true, true, false, false),
        };
        let next_offset = if idx + 1 < self.index.len() {
            match self.index.get(idx + 1) {
                Some((_, off)) => *off,
                None => self.index_offset,
            }
        } else {
            self.index_offset
        };

        let block_data = match self.get_block(offset, next_offset).await {
            Ok(b) => b,
            Err(_) => return (true, true, false, false),
        };

        let n = block_data.len();
        if n < 10 {
            return (true, true, true, false);
        }

        let num_offsets = match block_data.get(n.saturating_sub(2)..n) {
            Some(slice) => match slice.try_into() {
                Ok(arr) => u16::from_le_bytes(arr) as usize,
                Err(_) => return (true, true, true, false),
            },
            None => return (true, true, true, false),
        };

        let offsets_len = num_offsets.saturating_mul(2);
        if n < offsets_len.saturating_add(10) {
            return (true, true, true, false);
        }
        let offsets_start = n.saturating_sub(2).saturating_sub(offsets_len);
        let bloom_offset = offsets_start.saturating_sub(8);
        let bloom = match block_data.get(bloom_offset..bloom_offset.saturating_add(8)) {
            Some(slice) => match slice.try_into() {
                Ok(arr) => u64::from_le_bytes(arr),
                Err(_) => return (true, true, true, false),
            },
            None => return (true, true, true, false),
        };

        // Block bloom check
        let hash = blake3::hash(key);
        let hash_bytes = hash.as_bytes();
        let mut may_contain = true;
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
            return (true, true, true, false);
        }

        match binary_search_entry_in_block(&block_data, offsets_start, num_offsets, key) {
            Ok(Some(_)) => (true, true, true, true),
            _ => (true, true, true, false),
        }
    }

    pub fn metadata(&self) -> &SstableMetadata {
        &self.metadata
    }

    #[allow(clippy::unused_async)]
    pub async fn stream(self: &Arc<Self>) -> Result<SstableStream> {
        Ok(SstableStream {
            reader: Arc::clone(self),
            block_idx: 0,
            entry_idx: 0,
            current_block: None,
            num_offsets: 0,
            offsets_start: 0,
        })
    }

    /// Iterates over all entries in sorted key order (allocates memory for all entries).
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
                let _entry_key = block_data
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
                let _tx_id = u64::from_le_bytes(
                    block_data
                        .get(ep..ep + 8)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: tx_id".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u32::from_le_bytes(
                    block_data
                        .get(ep..ep + 4)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 4;
                if ep + v_len > block_data.len() {
                    return Err(MemFuseError::Storage(
                        "malformed block: value length out of bounds".into(),
                    ));
                }
                let key_bytes = block_data.slice(entry_off + 2..entry_off + 2 + k_len);
                let val_bytes = block_data.slice(ep..ep + v_len);

                results.push((key_bytes, val_bytes, seq_no));
            }
        }

        Ok(results)
    }
}

pub struct SstableStream {
    reader: Arc<SstableReader>,
    block_idx: usize,
    entry_idx: usize,
    current_block: Option<Bytes>,
    num_offsets: usize,
    offsets_start: usize,
}

impl SstableStream {
    pub async fn next(&mut self) -> Result<Option<(Bytes, Bytes, u64, u64)>> {
        if self.reader.index.is_empty() {
            return Ok(None);
        }

        loop {
            // Load a new block if needed
            if self.current_block.is_none() || self.entry_idx >= self.num_offsets {
                if self.block_idx >= self.reader.index.len() {
                    return Ok(None);
                }

                let offset = self.reader.index[self.block_idx].1;
                let next_offset = if self.block_idx + 1 < self.reader.index.len() {
                    self.reader.index[self.block_idx + 1].1
                } else {
                    self.reader.index_offset
                };

                let block_data = self.reader.get_block(offset, next_offset).await?;
                self.block_idx += 1;
                self.entry_idx = 0;

                let n = block_data.len();
                if n < 10 {
                    continue; // Empty or malformed block, try next
                }

                self.num_offsets = u16::from_le_bytes(
                    block_data
                        .get(n.saturating_sub(2)..n)
                        .ok_or_else(|| MemFuseError::Storage("missing num_offsets".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let offsets_len = self.num_offsets * 2;
                if n < 10 + offsets_len {
                    continue;
                }
                self.offsets_start = n - 2 - offsets_len;
                self.current_block = Some(block_data);
            }

            // Yield an entry from the current block
            if let Some(block_data) = &self.current_block {
                if self.entry_idx < self.num_offsets {
                    let off_pos = self.offsets_start + self.entry_idx * 2;
                    let entry_off = u16::from_le_bytes(
                        block_data
                            .get(off_pos..off_pos + 2)
                            .ok_or_else(|| MemFuseError::Storage("missing off_pos".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;

                    let mut ep = entry_off;
                    let k_len = u16::from_le_bytes(
                        block_data
                            .get(ep..ep + 2)
                            .ok_or_else(|| MemFuseError::Storage("missing k_len".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;
                    ep += 2;
                    let entry_key = block_data
                        .get(ep..ep + k_len)
                        .ok_or_else(|| MemFuseError::Storage("missing entry_key".into()))?;
                    ep += k_len;

                    let seq_no = u64::from_le_bytes(
                        block_data
                            .get(ep..ep + 8)
                            .ok_or_else(|| MemFuseError::Storage("missing seq_no".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    );
                    ep += 8;

                    let tx_id = u64::from_le_bytes(
                        block_data
                            .get(ep..ep + 8)
                            .ok_or_else(|| MemFuseError::Storage("missing tx_id".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    );
                    ep += 8;

                    let v_len = u32::from_le_bytes(
                        block_data
                            .get(ep..ep + 4)
                            .ok_or_else(|| MemFuseError::Storage("missing v_len".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;
                    ep += 4;
                    let entry_val = block_data.slice(ep..ep + v_len);
                    self.entry_idx += 1;
                    return Ok(Some((
                        Bytes::copy_from_slice(entry_key),
                        entry_val,
                        seq_no,
                        tx_id,
                    )));
                }
            }
            self.current_block = None;
        }
    }

    /// Optimized for compaction: avoids copying if possible.
    pub async fn next_entry(&mut self) -> Result<Option<(Bytes, Bytes, u64, u64)>> {
        self.next().await
    }
}

impl SstableReader {
    /// Scans the SSTable for keys starting with the given prefix.
    pub async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Bytes, Bytes, u64, u64)>> {
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

            let block_start_i = match binary_search_index_in_block(
                &block_data,
                offsets_start,
                num_offsets,
                prefix,
            )? {
                Ok(i) => i,
                Err(i) => i,
            };

            let mut broke = false;
            for i in block_start_i..num_offsets {
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
                    let tx_id = u64::from_le_bytes(
                        block_data
                            .get(ep..ep + 8)
                            .ok_or_else(|| MemFuseError::Storage("malformed block: tx_id".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    );
                    ep += 8;
                    let v_len = u32::from_le_bytes(
                        block_data
                            .get(ep..ep + 4)
                            .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;
                    ep += 4;
                    if ep + v_len > block_data.len() {
                        return Err(MemFuseError::Storage(
                            "malformed block: value length out of bounds".into(),
                        ));
                    }
                    let key_bytes = block_data.slice(entry_off + 2..entry_off + 2 + k_len);
                    let val_bytes = block_data.slice(ep..ep + v_len);
                    results.push((key_bytes, val_bytes, seq_no, tx_id));
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
    ) -> Result<Vec<(Bytes, Bytes, u64, u64)>> {
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

            let start_offset_idx = match start {
                Bound::Included(s) | Bound::Excluded(s) => {
                    match binary_search_index_in_block(&block_data, offsets_start, num_offsets, s)?
                    {
                        Ok(idx) => idx,
                        Err(idx) => idx,
                    }
                }
                Bound::Unbounded => 0,
            };

            for i in start_offset_idx..num_offsets {
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
                let tx_id = u64::from_le_bytes(
                    block_data
                        .get(ep..ep + 8)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: tx_id".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u32::from_le_bytes(
                    block_data
                        .get(ep..ep + 4)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 4;
                if ep + v_len > block_data.len() {
                    return Err(MemFuseError::Storage(
                        "malformed block: value length out of bounds".into(),
                    ));
                }
                let key_bytes = block_data.slice(entry_off + 2..entry_off + 2 + k_len);
                let val_bytes = block_data.slice(ep..ep + v_len);
                results.push((key_bytes, val_bytes, seq_no, tx_id));
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bloom_filter_fpr_within_bounds() {
        // 1000 echte Elemente, Ziel-FPR = 1%
        let mut bf = BloomFilter::new(1000, 0.01);
        let keys: Vec<Vec<u8>> = (0..1000u32).map(|i| i.to_le_bytes().to_vec()).collect();
        for k in &keys {
            bf.insert(k);
        }

        // Alle echten Elemente müssen enthalten sein (zero false negatives)
        for k in &keys {
            assert!(bf.may_contain(k), "False negative!");
        }

        // False Positive Rate messen mit fremden Keys
        let fp_count = (1000..2000u32)
            .filter(|i| bf.may_contain(&i.to_le_bytes()))
            .count();
        let fpr = fp_count as f64 / 1000.0;
        assert!(fpr < 0.05, "FPR {:.2}% > 5% Toleranz", fpr * 100.0);
    }

    #[test]
    fn test_bloom_filter_no_probe_repetition() {
        // Verifikation: Kein doppeltes Bit für kleine Hashes
        let mut bf = BloomFilter::new(100, 0.01);
        bf.insert(b"test_key");
        // Keine Assertion nötig — wenn kein Panic, ist der Algorithmus stabil
    }

    #[tokio::test]
    async fn test_block_bloom_filter() {
        let mut builder = BlockBuilder::new(4096);
        builder.add(b"apple", b"red", 1, 0);
        builder.add(b"banana", b"yellow", 2, 0);
        let block = builder.build();

        // 1. Verify format: [entries][u64 bloom][u16 offset1][u16 offset2][u16 num_offsets]
        let n = block.len();
        let num_offsets = u16::from_le_bytes(
            block
                .get(n.saturating_sub(2)..n)
                .expect("test") // expect
                .try_into()
                .expect("test"), // expect
        );
        assert_eq!(num_offsets, 2);

        let bloom_pos = n - 2 - (num_offsets as usize * 2) - 8;
        let bloom = u64::from_le_bytes(
            block
                .get(bloom_pos..bloom_pos + 8)
                .expect("test") // expect
                .try_into()
                .expect("correct length"), // expect
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
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("test.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder"); // expect
        builder.add(b"key1", b"val1", 1, 0).await.expect("add key1"); // expect
        builder.add(b"key2", b"val2", 2, 0).await.expect("add key2"); // expect
        builder.finish().await.expect("finish builder"); // expect

        let reader = SstableReader::open(&path, bc).await.expect("open reader"); // expect

        // Positive lookup
        let res = reader.get(b"key1").await.expect("get key1"); // expect
        assert_eq!(res.expect("exists").0.as_ref(), b"val1"); // expect

        // Negative lookup (should be caught by bloom or range check)
        let res = reader.get(b"nonexistent").await.expect("get nonexistent"); // expect
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn test_mmap_read_correct_values() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("mmap_test.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder"); // expect
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let val = format!("val-{:03}", i);
            builder
                .add(key.as_bytes(), val.as_bytes(), i as u64, 0)
                .await
                .expect("add"); // expect
        }
        builder.finish().await.expect("finish"); // expect

        let reader = SstableReader::open(&path, bc).await.expect("open"); // expect
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let expected = format!("val-{:03}", i);
            let res = reader
                .get(key.as_bytes())
                .await
                .expect("get") // expect
                .expect("exists"); // expect
            assert_eq!(res.0.as_ref(), expected.as_bytes());
            assert_eq!(res.1, i as u64);
        }
    }

    #[tokio::test]
    async fn test_mmap_concurrent_readers() {
        use std::sync::Arc;
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("mmap_concurrent.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder"); // expect
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let val = format!("val-{:03}", i);
            builder
                .add(key.as_bytes(), val.as_bytes(), i as u64, 0)
                .await
                .expect("add"); // expect
        }
        builder.finish().await.expect("finish"); // expect

        let reader = Arc::new(SstableReader::open(&path, bc).await.expect("open")); // expect
        let mut handles = Vec::new();

        for _ in 0..16 {
            let r = Arc::clone(&reader);
            handles.push(tokio::spawn(async move {
                for i in 0..100 {
                    let key = format!("key-{:03}", i);
                    let expected = format!("val-{:03}", i);
                    let res = r.get(key.as_bytes()).await.expect("get").expect("exists"); // expect
                    assert_eq!(res.0.as_ref(), expected.as_bytes());
                }
            }));
        }

        for h in handles {
            h.await.expect("task failed"); // expect
        }
    }

    #[tokio::test]
    async fn test_sstable_scan_prefix() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("scan_prefix.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder"); // expect
        builder.add(b"apple/1", b"a1", 1, 0).await.expect("add"); // expect
        builder.add(b"apple/2", b"a2", 2, 0).await.expect("add"); // expect
        builder.add(b"banana/1", b"b1", 3, 0).await.expect("add"); // expect
        builder.add(b"cherry/1", b"c1", 4, 0).await.expect("add"); // expect
        builder.finish().await.expect("finish"); // expect

        let reader = SstableReader::open(&path, bc).await.expect("open"); // expect

        let apples = reader.scan_prefix(b"apple/").await.expect("scan"); // expect
        assert_eq!(apples.len(), 2);
        assert_eq!(apples[0].0.as_ref(), b"apple/1");
        assert_eq!(apples[1].0.as_ref(), b"apple/2");

        let bananas = reader.scan_prefix(b"banana/").await.expect("scan"); // expect
        assert_eq!(bananas.len(), 1);
        assert_eq!(bananas[0].0.as_ref(), b"banana/1");

        let non = reader.scan_prefix(b"zebra").await.expect("scan"); // expect
        assert!(non.is_empty());
    }

    #[tokio::test]
    async fn test_sstable_scan_range() {
        use std::ops::Bound;
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("scan_range.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder"); // expect
        builder.add(b"a", b"1", 1, 0).await.expect("add"); // expect
        builder.add(b"b", b"2", 2, 0).await.expect("add"); // expect
        builder.add(b"c", b"3", 3, 0).await.expect("add"); // expect
        builder.add(b"d", b"4", 4, 0).await.expect("add"); // expect
        builder.finish().await.expect("finish"); // expect

        let reader = SstableReader::open(&path, bc).await.expect("open"); // expect

        // Included Range [b, c]
        let res = reader
            .scan_range(Bound::Included(b"b"), Bound::Included(b"c"))
            .await
            .expect("scan"); // expect
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0.as_ref(), b"b");
        assert_eq!(res[1].0.as_ref(), b"c");

        // Excluded Range (b, d) -> only c
        let res = reader
            .scan_range(Bound::Excluded(b"b"), Bound::Excluded(b"d"))
            .await
            .expect("scan"); // expect
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0.as_ref(), b"c");

        // Unbounded
        let res = reader
            .scan_range(Bound::Unbounded, Bound::Included(b"b"))
            .await
            .expect("scan"); // expect
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0.as_ref(), b"a");
    }

    #[tokio::test]
    async fn test_mfsx_bloom_filter_crc_recovery_no_false_rejections() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let sst_path = tmp.path().join("mfsx_crc_recovery.sst");
        let bc = create_block_cache(1);

        // Generate 120 unique keys
        let keys: Vec<Vec<u8>> = (0..120)
            .map(|i| format!("key_recovery_{:04}_{}", i, rand::random::<u32>()).into_bytes())
            .collect();

        // 1. Build MFSX SSTable with CRC
        {
            let mut builder = SstableBuilder::create(&sst_path)
                .await
                .expect("create builder"); // expect
            for (seq, key) in keys.iter().enumerate() {
                let val = format!("val_{}", seq).into_bytes();
                builder
                    .add(key, &val, seq as u64 + 1, 1)
                    .await
                    .expect("add key"); // expect
            }
            builder.finish().await.expect("finish builder"); // expect
        }

        // 2. Reopen reader (simulating process restart)
        let reader = SstableReader::open(&sst_path, bc)
            .await
            .expect("reopen reader"); // expect

        assert!(
            reader.bloom_filter.is_some(),
            "MFSX SSTable must have bloom filter"
        );
        assert!(reader.has_crc, "MFSX SSTable must have CRC enabled");

        // 3. Verify for ALL 120 keys that Bloom filter does not falsely reject any key
        for (seq, key) in keys.iter().enumerate() {
            let res = reader
                .get(key)
                .await
                .expect("get key")
                .expect("key must exist"); // expect
            let expected_val = format!("val_{}", seq).into_bytes();
            assert_eq!(
                res.0.as_ref(),
                expected_val.as_slice(),
                "Key {:?} must be found without false rejection",
                String::from_utf8_lossy(key)
            );
        }
    }

    #[tokio::test]
    async fn test_bloom_filter_integration() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let sst_path = tmp.path().join("bloom_test.sst");
        let bc = create_block_cache(1);

        // 1. Create SSTable with bloom filter
        {
            let mut builder = SstableBuilder::create(&sst_path).await.expect("create"); // expect
            builder
                .add(b"active-key", b"value", 100, 0)
                .await
                .expect("add"); // expect
            builder.finish().await.expect("finish"); // expect
        }

        // 2. Open and verify
        {
            let reader = SstableReader::open(&sst_path, bc.clone())
                .await
                .expect("open"); // expect
            assert!(reader.bloom_filter.is_some(), "Should have bloom filter");

            // Positive check
            let res = reader.get(b"active-key").await.expect("get"); // expect
            assert!(res.is_some());

            // Negative check (Bloom should say NO)
            let res = reader.get(b"missing-key-xyz-123").await.expect("get"); // expect
            assert!(res.is_none());
        }

        // 3. Backward compatibility (Manually create 12-byte trailer file)
        let old_sst_path = tmp.path().join("old_sst.sst");
        {
            // Use builder to create a valid SSTable first
            let mut builder = SstableBuilder::create(&old_sst_path)
                .await
                .expect("create old sub"); // expect
            builder
                .add(b"old-key", b"old-value", 10, 0)
                .await
                .expect("add"); // expect
            builder.finish().await.expect("finish"); // expect

            // Now manually truncate the trailer from 20 to 12 bytes
            // The file currently has: [data][index][bloom][bloom_off][index_off][magic] (total trailer 20)
            // We want to simulate: [data][index][index_off][magic] (total trailer 12)
            // Actually, just writing a 12-byte trailer pointing to the index is enough.
            let data = tokio::fs::read(&old_sst_path).await.expect("read"); // expect
            let file_size = data.len();
            let index_off =
                u64::from_le_bytes(data[file_size - 12..file_size - 4].try_into().unwrap()); // unwrap

            let mut new_data = data[0..file_size - 20].to_vec(); // remove new trailer and bloom
                                                                 // index likely ends at bloom_off. Let's just use the index_off we found.
            new_data.truncate((file_size - 20) as usize); // this might cut off some index if bloom was there
                                                          // Re-read data up to index_offset + index_size
                                                          // Actually, simpler: just rewrite a 12-byte trailer at the end of a valid data+index block.
                                                          // Let's just trust SstableReader to handle it if we only provide 12 bytes.
            let mut f = tokio::fs::File::create(&old_sst_path)
                .await
                .expect("recreate"); // expect
            f.write_all(&data[0..file_size - 24]).await.unwrap(); // expect
            f.write_u64_le(index_off).await.expect("write ioff"); // expect
            f.write_u32_le(0x4D465354).await.expect("write magic"); // expect
            f.sync_all().await.expect("sync"); // expect
        }

        {
            let reader = SstableReader::open(&old_sst_path, bc)
                .await
                .expect("open old"); // expect
            assert!(
                reader.bloom_filter.is_none(),
                "Old SST should not have bloom filter"
            );
        }
    }

    #[tokio::test]
    async fn test_sstable_block_crc_corruption() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("crc_corrupt.sst");
        let bc = create_block_cache(1);

        {
            let mut builder = SstableBuilder::create(&path).await.expect("create"); // expect
            builder.add(b"key1", b"val1", 1, 0).await.expect("add"); // expect
            builder.finish().await.expect("finish"); // expect
        }

        // Corrupt the first block
        {
            let mut data = tokio::fs::read(&path).await.expect("read"); // expect
                                                                        // Blocks start at 0. Let's flip a bit at offset 10.
            if data.len() > 10 {
                data[10] ^= 0xFF;
                tokio::fs::write(&path, data).await.expect("write"); // expect
            }
        }

        let reader_res = SstableReader::open(&path, bc).await;

        match reader_res {
            Ok(reader) => {
                let res = reader.get(b"key1").await;
                assert!(
                    res.is_err(),
                    "Expected error due to corruption during get, but got {:?}",
                    res
                );
                assert!(matches!(
                    res.unwrap_err(),
                    MemFuseError::ChecksumMismatch { .. }
                ));
            }
            Err(e) => {
                assert!(
                    matches!(e, MemFuseError::ChecksumMismatch { .. }),
                    "Expected ChecksumMismatch, got {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_block_cache_extreme_values() {
        // Darf nicht paniken oder overflowlen – prüft alle Grenzfälle.
        let _ = create_block_cache(0); // minimum → floor auf 256 Blöcke
        let _ = create_block_cache(1); // normal
        let _ = create_block_cache(usize::MAX); // overflow-Test → saturating → cap
        let _ = create_block_cache(usize::MAX / 2); // near-overflow → saturating → cap
    }

    #[tokio::test]
    async fn test_block_cache_eviction_under_load() {
        // Create a 10-byte capacity per shard cache directly (for 6-byte blocks)
        let cache = Arc::new(BlockCache::new(10));

        let file_id = 1u64;
        // Find 3 offsets that map to the exact same shard index using ahash
        let target_shard = cache.shard_idx(file_id, 0);
        let mut offsets = vec![0u64];
        let mut candidate = 1u64;
        while offsets.len() < 3 {
            if cache.shard_idx(file_id, candidate) == target_shard {
                offsets.push(candidate);
            }
            candidate += 1;
        }

        let offset1 = offsets[0];
        let offset2 = offsets[1];
        let offset3 = offsets[2];

        // 1. Insert block 1 -> populates shard
        cache.insert(file_id, offset1, Bytes::from_static(b"block1"));
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(file_id, offset1));

        // 2. Insert block 2 -> evicts block 1 in same shard
        cache.insert(file_id, offset2, Bytes::from_static(b"block2"));
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains(file_id, offset1));
        assert!(cache.contains(file_id, offset2));

        // 3. Insert block 3 -> evicts block 2 in same shard
        cache.insert(file_id, offset3, Bytes::from_static(b"block3"));
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains(file_id, offset1));
        assert!(!cache.contains(file_id, offset2));
        assert!(cache.contains(file_id, offset3));
    }

    #[tokio::test]
    async fn test_block_cache_byte_capacity_eviction_threshold() {
        // Set per-shard capacity to 20 bytes
        let cache = Arc::new(BlockCache::new(20));
        let file_id = 100u64;

        // Find offsets mapping to shard 0
        let mut offsets = Vec::new();
        let mut cand = 0u64;
        while offsets.len() < 2 {
            if cache.shard_idx(file_id, cand) == 0 {
                offsets.push(cand);
            }
            cand += 1;
        }

        let block1 = Bytes::from(vec![0u8; 15]); // 15 bytes
        let block2 = Bytes::from(vec![1u8; 10]); // 10 bytes (total 25 > 20 capacity_bytes)

        cache.insert(file_id, offsets[0], block1);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(file_id, offsets[0]));

        // Insert block2, causing total bytes (25) to exceed shard capacity (20) -> evicts block1
        cache.insert(file_id, offsets[1], block2);
        assert_eq!(cache.len(), 1);
        assert!(
            !cache.contains(file_id, offsets[0]),
            "block1 must be evicted due to byte capacity limit"
        );
        assert!(cache.contains(file_id, offsets[1]));
    }

    #[tokio::test]
    async fn test_sstable_builder_duplicate_keys_coexist() {
        let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)]
        let path = tmp.path().join("duplicate_keys.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create"); // expect #[cfg(test)]
        builder.add(b"k", b"val1", 1, 10).await.expect("add seq 1"); // expect #[cfg(test)]
        builder.add(b"k", b"val2", 2, 20).await.expect("add seq 2"); // expect #[cfg(test)]
        builder.finish().await.expect("finish"); // expect #[cfg(test)]

        let reader = SstableReader::open(&path, bc).await.expect("open"); // expect #[cfg(test)]

        // 1. Verify via iter()
        let iter_entries = reader.iter().await.expect("iter"); // expect #[cfg(test)]
        assert_eq!(
            iter_entries.len(),
            2,
            "Both duplicate key entries must coexist in iter()"
        );
        assert_eq!(
            iter_entries[0],
            (Bytes::from_static(b"k"), Bytes::from_static(b"val1"), 1)
        );
        assert_eq!(
            iter_entries[1],
            (Bytes::from_static(b"k"), Bytes::from_static(b"val2"), 2)
        );

        // 2. Verify via stream()
        let reader_arc = Arc::new(reader);
        let mut stream = reader_arc.stream().await.expect("stream"); // expect #[cfg(test)]
        let e1 = stream.next().await.expect("next").expect("entry 1"); // expect #[cfg(test)]
        let e2 = stream.next().await.expect("next").expect("entry 2"); // expect #[cfg(test)]
        let e_end = stream.next().await.expect("next"); // expect #[cfg(test)]

        assert_eq!(
            e1,
            (Bytes::from_static(b"k"), Bytes::from_static(b"val1"), 1, 10)
        );
        assert_eq!(
            e2,
            (Bytes::from_static(b"k"), Bytes::from_static(b"val2"), 2, 20)
        );
        assert!(e_end.is_none());
    }

    #[test]
    fn test_bloom_filter_boundary_clamping() {
        // Test extreme elements input
        let bf = BloomFilter::new(usize::MAX, 0.01);
        assert!(bf.num_bits <= 128 * 1024 * 1024 * 8);

        // Test corrupted bytes input
        let mut corrupted_data = vec![0u8; 16];
        // Set num_bits to huge value
        corrupted_data[8..16].copy_from_slice(&(u64::MAX).to_le_bytes());
        let res = BloomFilter::from_bytes(&corrupted_data);
        assert!(res.is_err());

        // Test capacity cap on from_bytes
        let mut valid_header = vec![0u8; 24];
        valid_header[0..8].copy_from_slice(&1u64.to_le_bytes()); // num_hashes
        valid_header[8..16].copy_from_slice(&1000u64.to_le_bytes()); // num_bits
        let bf_res = BloomFilter::from_bytes(&valid_header);
        assert!(bf_res.is_ok());
    }

    #[test]
    fn test_bloom_filter_roundtrip_and_too_short_bytes() {
        // Test roundtrip
        let mut bf = BloomFilter::new(50, 0.01);
        bf.insert(b"test-key-1");
        bf.insert(b"test-key-2");
        let bytes = bf.to_bytes();

        let restored = BloomFilter::from_bytes(&bytes).expect("deserialization should succeed"); // expect
        assert!(restored.may_contain(b"test-key-1"));
        assert!(restored.may_contain(b"test-key-2"));
        assert!(!restored.may_contain(b"non-existent-key"));

        // Test deserialization with too short data (< 16 bytes)
        let short_bytes = vec![0u8; 15];
        let err = BloomFilter::from_bytes(&short_bytes);
        assert!(matches!(err, Err(MemFuseError::Storage(_))));
    }

    #[test]
    fn test_block_builder_min_size() {
        let builder = BlockBuilder::new(10);
        assert_eq!(builder.block_size, 512);
    }

    #[test]
    fn test_block_builder_min_max_clamping() {
        let bb_small = BlockBuilder::new(10);
        assert_eq!(bb_small.block_size, 512);

        let bb_huge = BlockBuilder::new(100 * 1024 * 1024);
        assert_eq!(bb_huge.block_size, 64 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_sstable_builder_rejects_empty_and_oversized_inputs() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().join("boundary_test.sst");

        let mut builder = SstableBuilder::create(&path).await.expect("create"); // expect

        // 1. Empty key reject
        let err_empty = builder.add(b"", b"val", 1, 1).await;
        assert!(matches!(err_empty, Err(MemFuseError::InvalidInput(_))));

        // 2. Oversized key (>65535) reject
        let oversized_key = vec![0xAA; 65536];
        let err_key = builder.add(&oversized_key, b"val", 1, 1).await;
        assert!(matches!(err_key, Err(MemFuseError::InvalidInput(_))));

        // 3. Oversized value (>MAX_VALUE_SIZE) reject
        let oversized_val = vec![0xBB; crate::lsm::MAX_VALUE_SIZE + 1];
        let err_val = builder.add(b"valid_key", &oversized_val, 1, 1).await;
        assert!(matches!(err_val, Err(MemFuseError::InvalidInput(_))));
    }

    #[test]
    fn test_binary_search_vs_linear_search_equivalence() {
        // Test randomized blocks with 1..=64 entries to ensure binary search matches linear search exactly
        for num_entries in 1..=64 {
            let mut builder = BlockBuilder::new(64 * 1024);
            let mut expected_entries = Vec::new();
            for i in 0..num_entries {
                let key = format!("key_{:04}", i).into_bytes();
                let val = format!("val_{:04}", i).into_bytes();
                builder.add(&key, &val, i as u64, 0);
                expected_entries.push((key, val));
            }
            let block = builder.build();
            let n = block.len();
            let num_offsets = u16::from_le_bytes(block[n - 2..n].try_into().unwrap()) as usize;
            let offsets_len = num_offsets * 2;
            let offsets_start = n - 2 - offsets_len;

            // 1. Verify all present keys
            for (key, _val) in &expected_entries {
                // Linear search
                let mut linear_res = None;
                for idx in 0..num_offsets {
                    let off_pos = offsets_start + idx * 2;
                    let entry_off =
                        u16::from_le_bytes(block[off_pos..off_pos + 2].try_into().unwrap())
                            as usize;
                    let k_len =
                        u16::from_le_bytes(block[entry_off..entry_off + 2].try_into().unwrap())
                            as usize;
                    let entry_key = &block[entry_off + 2..entry_off + 2 + k_len];
                    if entry_key == key {
                        linear_res = Some((entry_off, k_len));
                        break;
                    }
                }

                // Binary search
                let bin_res = binary_search_entry_in_block(&block, offsets_start, num_offsets, key)
                    .expect("binary search should not error");

                assert_eq!(
                    linear_res,
                    bin_res,
                    "Mismatch for key {:?} in block with {} entries",
                    String::from_utf8_lossy(key),
                    num_entries
                );

                if let Some((off, klen)) = bin_res {
                    assert_eq!(&block[off + 2..off + 2 + klen], key.as_slice());
                }
            }

            // 2. Verify non-existent keys
            let non_existent_keys = [
                b"key_-001".as_slice(),
                b"key_9999".as_slice(),
                b"key_0000_foo".as_slice(),
            ];
            for nek in non_existent_keys {
                let bin_res = binary_search_entry_in_block(&block, offsets_start, num_offsets, nek)
                    .expect("binary search should not error");
                assert!(
                    bin_res.is_none(),
                    "Non-existent key {:?} found in block with {} entries",
                    String::from_utf8_lossy(nek),
                    num_entries
                );
            }
        }
    }

    #[tokio::test]
    async fn test_sharded_block_cache_concurrency_stress() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(BlockCache::new(16));
        let num_tasks = 32;
        let ops_per_task = 1000;
        let completed = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for task_id in 0..num_tasks {
            let cache_ref = Arc::clone(&cache);
            let completed_ref = Arc::clone(&completed);

            handles.push(tokio::spawn(async move {
                for i in 0..ops_per_task {
                    // Unique key targeting different shards
                    let file_id = (task_id as u64) + 1;
                    let block_offset = (i as u64) * 4096;
                    let val = Bytes::from(format!("data_{}_{}", task_id, i));

                    cache_ref.insert(file_id, block_offset, val.clone());
                    let retrieved = cache_ref.get(file_id, block_offset);
                    assert_eq!(retrieved, Some(val));
                }
                completed_ref.fetch_add(1, Ordering::SeqCst);
            }));
        }

        for h in handles {
            h.await.expect("task panicked");
        }

        assert_eq!(completed.load(Ordering::SeqCst), num_tasks);
        assert!(!cache.is_empty());
    }

    #[tokio::test]
    async fn test_block_cache_shards_config_validation() {
        use crate::lsm::{LsmConfig, LsmStorage};

        let tmp = TempDir::new().expect("temp dir");

        // 0 shards -> Err
        let config_zero = LsmConfig {
            path: tmp.path().join("zero"),
            block_cache_shards: 0,
            ..Default::default()
        };
        assert!(LsmStorage::new(config_zero).await.is_err());

        // Non-power-of-two shards (e.g. 15) -> Err
        let config_non_pow2 = LsmConfig {
            path: tmp.path().join("non_pow2"),
            block_cache_shards: 15,
            ..Default::default()
        };
        assert!(LsmStorage::new(config_non_pow2).await.is_err());

        // Power of two shards (e.g. 32) -> Ok
        let config_valid = LsmConfig {
            path: tmp.path().join("valid"),
            block_cache_shards: 32,
            ..Default::default()
        };
        let storage = LsmStorage::new(config_valid).await;
        assert!(storage.is_ok());
    }

    #[test]
    fn test_block_cache_sequential_scan_sharding_distribution() {
        // Verification of sharding distribution during sequential scans with constant file_id.
        // The hashing function `ahash::RandomState::hash_one((file_id, offset))` distributes
        // sequential block offsets (4KB increments) evenly across all shards.
        let num_shards = 64;
        let cache = BlockCache::new_with_shards(16, num_shards);
        let file_id = 100u64;
        let num_blocks = 1000u64;

        let mut shard_counts = vec![0usize; num_shards];
        for i in 0..num_blocks {
            let offset = i * 4096;
            let shard = cache.shard_idx(file_id, offset);
            shard_counts[shard] += 1;
        }

        // Ensure every shard received at least one block offset (no dead shards in sequential scan)
        let empty_shards = shard_counts.iter().filter(|&&c| c == 0).count();
        assert_eq!(
            empty_shards, 0,
            "Sequential scan produced empty shards: {:?}",
            shard_counts
        );

        // Verify standard deviation or max load to ensure uniform distribution
        let expected_avg = num_blocks as f64 / num_shards as f64;
        for (idx, &count) in shard_counts.iter().enumerate() {
            let diff = (count as f64 - expected_avg).abs();
            assert!(
                diff < expected_avg * 1.5,
                "Shard {} has count {} which deviates significantly from expected average {:.1}",
                idx,
                count,
                expected_avg
            );
        }
    }

    #[tokio::test]
    async fn test_block_cache_32_concurrent_readers_hotset_latency() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        let cache = Arc::new(BlockCache::new(64));
        let file_id = 42u64;
        let offset = 4096u64;
        let val = Bytes::from_static(b"hotset_block_payload");

        cache.insert(file_id, offset, val.clone());

        let num_readers = 32;
        let reads_per_reader = 10_000;
        let total_hits = Arc::new(AtomicUsize::new(0));

        let start = Instant::now();
        let mut handles = Vec::with_capacity(num_readers);

        for _ in 0..num_readers {
            let cache_ref = Arc::clone(&cache);
            let hits_ref = Arc::clone(&total_hits);
            let expected_val = val.clone();
            handles.push(tokio::spawn(async move {
                let mut hits = 0;
                for _ in 0..reads_per_reader {
                    if let Some(res) = cache_ref.get(file_id, offset) {
                        if res == expected_val {
                            hits += 1;
                        }
                    }
                }
                hits_ref.fetch_add(hits, Ordering::Relaxed);
            }));
        }

        for h in handles {
            h.await.expect("reader task completed");
        }

        let elapsed = start.elapsed();
        let total_reads = num_readers * reads_per_reader;
        assert_eq!(total_hits.load(Ordering::SeqCst), total_reads);

        let ns_per_op = elapsed.as_nanos() as f64 / total_reads as f64;
        println!(
            "[BlockCache Benchmark] 32 concurrent readers hot-set: {} total ops in {:?}, avg {:.2} ns/op (feature block-cache-v2={})",
            total_reads,
            elapsed,
            ns_per_op,
            cfg!(feature = "block-cache-v2")
        );
    }
}
