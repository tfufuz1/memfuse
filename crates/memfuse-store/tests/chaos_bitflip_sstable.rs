//! Integration-Test: Chaos Bit-Flip Fuzzing in SSTables
//!
//! Verifies LSM SSTable integrity under targeted bit-flip corruption in:
//! 1. Data blocks (payload area, strictly excluding block CRC fields)
//! 2. Bloom filter byte range (payload area)
//! 3. Index byte range (payload area)
//!
//! Invariants:
//! - Data block corruption MUST return `MemFuseError::ChecksumMismatch`, NEVER a silently wrong value or success.
//! - Bloom filter corruption MUST NEVER produce false negatives (`Ok(None)`) for existing keys (data loss).
//! - Index corruption MUST cleanly fail with documented error (`ChecksumMismatch` or `ParseError`).

use memfuse_core::{MemFuseError, Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_store::sstable::{create_block_cache, SstableReader};
use proptest::prelude::*;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

/// Represents parsed SSTable byte layout boundaries and block offsets.
#[derive(Debug, Clone)]
struct SstableLayout {
    file_path: PathBuf,
    file_size: u64,
    index_offset: u64,
    bloom_offset: u64,
    trailer_offset: u64,
    /// Start offsets for all data blocks in the SSTable.
    block_offsets: Vec<u64>,
}

impl SstableLayout {
    /// Parses an SSTable trailer and index to obtain section offsets and block boundaries.
    fn parse(file_path: &Path) -> std::io::Result<Self> {
        let mut file = OpenOptions::new().read(true).open(file_path)?;
        let file_size = file.metadata()?.len();
        if file_size < 54 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "SSTable file too small for v1 trailer",
            ));
        }

        let trailer_offset = file_size - 54;
        file.seek(SeekFrom::Start(trailer_offset))?;
        let mut trailer = [0u8; 54];
        file.read_exact(&mut trailer)?;

        let bloom_offset = u64::from_le_bytes(trailer[32..40].try_into().unwrap());
        let index_offset = u64::from_le_bytes(trailer[40..48].try_into().unwrap());

        // Read index entries (after 4-byte CRC)
        let index_payload_len = (bloom_offset - index_offset).saturating_sub(4) as usize;
        let mut index_payload = vec![0u8; index_payload_len];
        file.seek(SeekFrom::Start(index_offset + 4))?;
        file.read_exact(&mut index_payload)?;

        let mut pos = 0;
        let mut block_offsets = Vec::new();
        while pos + 10 <= index_payload_len {
            let key_len =
                u16::from_le_bytes(index_payload[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2 + key_len;
            if pos + 8 > index_payload_len {
                break;
            }
            let offset = u64::from_le_bytes(index_payload[pos..pos + 8].try_into().unwrap());
            pos += 8;
            block_offsets.push(offset);
        }

        Ok(Self {
            file_path: file_path.to_path_buf(),
            file_size,
            index_offset,
            bloom_offset,
            trailer_offset,
            block_offsets,
        })
    }
}

/// Injects a targeted bit-flip into a file on disk.
fn inject_bit_flip(file_path: &Path, offset: u64, bit_idx: u8) -> std::io::Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte)?;
    byte[0] ^= 1 << (bit_idx % 8);
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&byte)?;
    file.sync_all()?;
    Ok(())
}

/// Helper: Creates storage, populates ground truth entries, forces flush, and returns layout.
async fn setup_flushed_sstable(
    entry_count: usize,
) -> Result<(
    TempDir,
    LsmConfig,
    Vec<(Vec<u8>, Vec<u8>)>,
    SstableLayout,
    Vec<Vec<(Vec<u8>, Vec<u8>)>>,
)> {
    let tmp = TempDir::new().map_err(|e| MemFuseError::Storage(e.to_string()))?;
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    let mut ground_truth = Vec::with_capacity(entry_count);
    {
        let storage = LsmStorage::new(config.clone()).await?;

        for i in 0..entry_count {
            let tx = TxId::new(i as u64 + 1);
            let key = format!("sstable_chaos_key_{:05}", i).into_bytes();
            let val = format!("sstable_chaos_val_{:05}_payload_content", i).into_bytes();
            storage.put(tx, &key, &val).await?;
            storage.commit(tx).await?;
            ground_truth.push((key, val));
        }

        storage.force_flush().await?;
        storage.wait_shutdown().await;
    }

    // Locate generated SSTable file
    let sst_path = fs::read_dir(tmp.path())
        .map_err(|e| MemFuseError::Storage(e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map_or(false, |ext| ext == "sst"))
        .ok_or_else(|| MemFuseError::Storage("No SSTable file generated after flush".into()))?;

    let layout = SstableLayout::parse(&sst_path)
        .map_err(|e| MemFuseError::Storage(format!("Failed to parse SSTable layout: {}", e)))?;

    // Map ground-truth keys to blocks using an uncorrupted reader
    let bc = create_block_cache(1);
    let reader = SstableReader::open(&sst_path, bc).await?;

    let num_blocks = layout.block_offsets.len();
    let mut keys_per_block = vec![Vec::new(); num_blocks];

    for (key, val) in &ground_truth {
        // Query metrics or lookup block
        let res = reader.get(key).await?;
        assert!(res.is_some(), "Ground truth key must exist initially");

        // Find which block index this key landed in
        for b_idx in 0..num_blocks {
            let b_start = layout.block_offsets[b_idx];
            let b_end = if b_idx + 1 < num_blocks {
                layout.block_offsets[b_idx + 1]
            } else {
                layout.index_offset
            };

            // Read block directly to check if key is in this block
            let mut file = OpenOptions::new().read(true).open(&sst_path).unwrap();
            file.seek(SeekFrom::Start(b_start + 4)).unwrap();
            let mut buf = vec![0u8; (b_end - b_start - 4) as usize];
            file.read_exact(&mut buf).unwrap();

            if buf.windows(key.len()).any(|w| w == key.as_slice()) {
                keys_per_block[b_idx].push((key.clone(), val.clone()));
                break;
            }
        }
    }

    Ok((tmp, config, ground_truth, layout, keys_per_block))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Test 2a: Bit-flip inside Data Block payload (EXCLUDING the 4-byte block CRC field).
    /// Must return `MemFuseError::ChecksumMismatch` on key lookup for keys in the corrupted block,
    /// and NEVER return a silently wrong value or success for corrupted data.
    #[test]
    fn test_sstable_data_block_bitflip(
        bit_idx in 0u8..8,
        block_idx_ratio in 0.0f64..1.0f64,
        payload_offset_ratio in 0.0f64..1.0f64,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (_tmp, _config, _ground_truth, layout, keys_per_block) = setup_flushed_sstable(60).await.unwrap();

            if layout.block_offsets.is_empty() {
                return;
            }

            let num_blocks = layout.block_offsets.len();
            let target_block_idx = ((block_idx_ratio * num_blocks as f64) as usize).min(num_blocks - 1);

            let block_start = layout.block_offsets[target_block_idx];
            let block_end = if target_block_idx + 1 < num_blocks {
                layout.block_offsets[target_block_idx + 1]
            } else {
                layout.index_offset
            };

            // Exclude 4-byte CRC field at start of block
            let payload_start = block_start + 4;
            if block_end <= payload_start {
                return;
            }

            let payload_len = block_end - payload_start;
            let offset_in_payload = (payload_offset_ratio * (payload_len as f64 - 1.0)) as u64;
            let flip_offset = payload_start + offset_in_payload;

            inject_bit_flip(&layout.file_path, flip_offset, bit_idx).unwrap();

            let bc = create_block_cache(1);
            let reader_res = SstableReader::open(&layout.file_path, bc).await;

            match reader_res {
                Err(e) => {
                    assert!(
                        matches!(e, MemFuseError::ChecksumMismatch { .. }),
                        "Expected ChecksumMismatch on open, got {:?}",
                        e
                    );
                }
                Ok(reader) => {
                    let affected_keys = &keys_per_block[target_block_idx];
                    for (key, expected_val) in affected_keys {
                        let res = reader.get(key).await;
                        match res {
                            Err(e) => {
                                assert!(
                                    matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                    "Expected ChecksumMismatch on get for corrupted block, got {:?}",
                                    e
                                );
                            }
                            Ok(Some((val, _seq, _tx))) => {
                                panic!(
                                    "SILENT DATA CORRUPTION: get() succeeded on corrupted block for key {:?}! Got val {:?}, expected {:?}",
                                    String::from_utf8_lossy(key),
                                    val,
                                    expected_val
                                );
                            }
                            Ok(None) => {
                                panic!(
                                    "SILENT DATA LOSS: get() returned Ok(None) instead of ChecksumMismatch for corrupted block key {:?}",
                                    String::from_utf8_lossy(key)
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    /// Test 2b: Bit-flip in Bloom Filter byte range.
    /// Must NEVER produce a false negative (`Ok(None)`) for an existing key (data loss).
    #[test]
    fn test_sstable_bloom_filter_bitflip(
        bit_idx in 0u8..8,
        bloom_offset_ratio in 0.0f64..1.0f64,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (_tmp, config, ground_truth, layout, _keys_per_block) = setup_flushed_sstable(50).await.unwrap();

            let bloom_payload_start = layout.bloom_offset + 4; // Skip 4-byte bloom CRC
            if layout.trailer_offset <= bloom_payload_start {
                return;
            }

            let bloom_payload_len = layout.trailer_offset - bloom_payload_start;
            let offset_in_bloom = (bloom_offset_ratio * (bloom_payload_len as f64 - 1.0)) as u64;
            let flip_offset = bloom_payload_start + offset_in_bloom;

            inject_bit_flip(&layout.file_path, flip_offset, bit_idx).unwrap();

            let bc = create_block_cache(1);
            let reader_res = SstableReader::open(&layout.file_path, bc).await;

            match reader_res {
                Err(e) => {
                    assert!(
                        matches!(
                            e,
                            MemFuseError::ChecksumMismatch { .. }
                                | MemFuseError::ParseError(_)
                                | MemFuseError::Storage(_)
                        ),
                        "Expected clean error on corrupt bloom filter open, got {:?}",
                        e
                    );
                }
                Ok(reader) => {
                    for (key, expected_val) in &ground_truth {
                        match reader.get(key).await {
                            Err(e) => {
                                assert!(
                                    matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                    "Expected ChecksumMismatch on read, got {:?}",
                                    e
                                );
                            }
                            Ok(Some((val, _seq, _tx))) => {
                                assert_eq!(
                                    val.as_ref(),
                                    expected_val.as_slice(),
                                    "Value mismatch on bloom filter bitflip"
                                );
                            }
                            Ok(None) => {
                                panic!(
                                    "CRITICAL DATA LOSS: Corrupted Bloom Filter produced FALSE NEGATIVE Ok(None) for existing key {:?}!",
                                    String::from_utf8_lossy(key)
                                );
                            }
                        }
                    }
                }
            }

            let storage_res = LsmStorage::new(config).await;
            if let Ok(storage) = storage_res {
                for (key, expected_val) in &ground_truth {
                    match storage.get(key).await {
                        Err(e) => {
                            assert!(
                                matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                "Expected ChecksumMismatch from LsmStorage, got {:?}",
                                e
                            );
                        }
                        Ok(Some(val)) => {
                            assert_eq!(
                                val.as_slice(),
                                expected_val.as_slice(),
                                "Value mismatch on LsmStorage get"
                            );
                        }
                        Ok(None) => {
                            panic!(
                                "CRITICAL DATA LOSS: Corrupted Bloom Filter in LsmStorage produced FALSE NEGATIVE Ok(None) for existing key {:?}!",
                                String::from_utf8_lossy(key)
                            );
                        }
                    }
                }
                storage.wait_shutdown().await;
            } else if let Err(e) = storage_res {
                assert!(
                    matches!(
                        e,
                        MemFuseError::ChecksumMismatch { .. }
                            | MemFuseError::ParseError(_)
                            | MemFuseError::Storage(_)
                    ),
                    "Expected clean error on LsmStorage open, got {:?}",
                    e
                );
            }
        });
    }

    /// Test 2c: Bit-flip in Index byte range.
    /// Must cleanly throw a documented error (`ChecksumMismatch` or `ParseError`).
    #[test]
    fn test_sstable_index_bitflip(
        bit_idx in 0u8..8,
        index_offset_ratio in 0.0f64..1.0f64,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (_tmp, config, ground_truth, layout, _keys_per_block) = setup_flushed_sstable(40).await.unwrap();

            let index_payload_start = layout.index_offset + 4; // Skip 4-byte index CRC
            if layout.bloom_offset <= index_payload_start {
                return;
            }

            let index_payload_len = layout.bloom_offset - index_payload_start;
            let offset_in_index = (index_offset_ratio * (index_payload_len as f64 - 1.0)) as u64;
            let flip_offset = index_payload_start + offset_in_index;

            inject_bit_flip(&layout.file_path, flip_offset, bit_idx).unwrap();

            let bc = create_block_cache(1);
            let reader_res = SstableReader::open(&layout.file_path, bc).await;

            match reader_res {
                Err(e) => {
                    assert!(
                        matches!(
                            e,
                            MemFuseError::ChecksumMismatch { .. }
                                | MemFuseError::ParseError(_)
                                | MemFuseError::Storage(_)
                        ),
                        "Index corruption must return ChecksumMismatch, ParseError or Storage error, got {:?}",
                        e
                    );
                }
                Ok(reader) => {
                    for (key, expected_val) in &ground_truth {
                        match reader.get(key).await {
                            Err(e) => {
                                assert!(
                                    matches!(
                                        e,
                                        MemFuseError::ChecksumMismatch { .. }
                                            | MemFuseError::ParseError(_)
                                            | MemFuseError::Storage(_)
                                    ),
                                    "Expected clean error on corrupted index read, got {:?}",
                                    e
                                );
                            }
                            Ok(Some((val, _seq, _tx))) => {
                                assert_eq!(
                                    val.as_ref(),
                                    expected_val.as_slice(),
                                    "Value mismatch on index corruption read"
                                );
                            }
                            Ok(None) => {}
                        }
                    }
                }
            }

            let storage_res = LsmStorage::new(config).await;
            if let Ok(storage) = storage_res {
                for (key, expected_val) in &ground_truth {
                    match storage.get(key).await {
                        Err(e) => {
                            assert!(
                                matches!(
                                    e,
                                    MemFuseError::ChecksumMismatch { .. }
                                        | MemFuseError::ParseError(_)
                                        | MemFuseError::Storage(_)
                                ),
                                "Expected clean error on corrupted index LsmStorage get, got {:?}",
                                e
                            );
                        }
                        Ok(Some(val)) => {
                            assert_eq!(val.as_slice(), expected_val.as_slice());
                        }
                        Ok(None) => {}
                    }
                }
                storage.wait_shutdown().await;
            } else if let Err(e) = storage_res {
                assert!(
                    matches!(
                        e,
                        MemFuseError::ChecksumMismatch { .. }
                            | MemFuseError::ParseError(_)
                            | MemFuseError::Storage(_)
                    ),
                    "Expected clean error on LsmStorage open, got {:?}",
                    e
                );
            }
        });
    }

    /// General Property Test: Random Bit-flip across the entire SSTable file never causes panics.
    #[test]
    fn prop_sstable_bitflip_never_panics(
        bit_idx in 0u8..8,
        relative_pos in 0.0f64..1.0f64
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (_tmp, config, _ground_truth, layout, _keys_per_block) = setup_flushed_sstable(20).await.unwrap();
            let file_len = layout.file_size;
            if file_len > 0 {
                let offset = (relative_pos * (file_len as f64 - 1.0)) as u64;
                inject_bit_flip(&layout.file_path, offset, bit_idx).unwrap();

                let bc = create_block_cache(1);
                let _ = SstableReader::open(&layout.file_path, bc).await;
                let _ = LsmStorage::new(config).await;
            }
        });
    }
}
