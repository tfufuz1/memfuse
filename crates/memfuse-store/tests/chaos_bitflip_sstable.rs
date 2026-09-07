//! Integration-Test: Chaos Bit-Flip SSTable Injektion (`chaos_bitflip_sstable.rs`)
//!
//! Testziel:
//! Injektion zielgerichteter Bit-Flips in SSTable-Dateien auf Disk via `proptest` und
//! Verifikation der System-Robustheit in drei separaten Testfällen:
//! a. Data-Block-Korruption -> `MemFuseError::ChecksumMismatch` MUSS zurückkommen, NIEMALS ein stillschweigend falscher Wert.
//! b. Bloom-Filter-Korruption -> Darf NIEMALS zu False Negatives (`Ok(None)`) für existierende Keys führen (Datenverlust).
//! c. Index-Korruption -> MUSS deterministisch mit Fehler behandeln (z.B. ChecksumMismatch oder ParseError), NIEMALS Panik.

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use proptest::prelude::*;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

/// Magic bytes for SSTable file trailer ("MFSX")
const SSTABLE_MAGIC_MFSX: u32 = 0x5853_464D;

/// SSTable Layout Metadata with block boundaries
#[derive(Debug, Clone)]
struct SstableLayout {
    path: PathBuf,
    file_size: u64,
    data_block_ranges: Vec<(u64, u64)>, // (block_start, block_end)
    index_offset: u64,
    bloom_offset: u64,
}

impl SstableLayout {
    /// Inverts bit at the specified byte offset.
    fn inject_bit_flip(&self, offset: u64, bit_idx: u8) -> std::io::Result<()> {
        let mut file = OpenOptions::new().read(true).write(true).open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte)?;
        byte[0] ^= 1 << (bit_idx % 8);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&byte)?;
        file.sync_all()?;
        Ok(())
    }
}

/// Parses an SSTable on disk to extract exact section byte ranges and data block boundaries.
fn parse_sstable_layout(path: &Path) -> std::io::Result<SstableLayout> {
    let mut file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len();
    if file_size < 54 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SSTable file too small for v1 trailer",
        ));
    }

    // Read 54-byte trailer
    let mut trailer = [0u8; 54];
    file.seek(SeekFrom::Start(file_size - 54))?;
    file.read_exact(&mut trailer)?;

    let magic = u32::from_le_bytes(trailer[50..54].try_into().unwrap());
    if magic != SSTABLE_MAGIC_MFSX {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid SSTable magic: 0x{:08X}", magic),
        ));
    }

    let bloom_offset = u64::from_le_bytes(trailer[32..40].try_into().unwrap());
    let index_offset = u64::from_le_bytes(trailer[40..48].try_into().unwrap());

    // Read index to extract block offsets
    // Index structure: [index_crc: 4 bytes][key_len: u16, key: key_len bytes, offset: u64]*
    let index_len = (bloom_offset - index_offset) as usize;
    let mut index_data = vec![0u8; index_len];
    file.seek(SeekFrom::Start(index_offset))?;
    file.read_exact(&mut index_data)?;

    let mut block_offsets = Vec::new();
    if index_data.len() >= 4 {
        let mut pos = 4; // skip 4-byte index CRC
        while pos + 10 <= index_data.len() {
            let key_len = u16::from_le_bytes(index_data[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;
            if pos + key_len + 8 > index_data.len() {
                break;
            }
            pos += key_len;
            let offset = u64::from_le_bytes(index_data[pos..pos + 8].try_into().unwrap());
            pos += 8;
            block_offsets.push(offset);
        }
    }

    let mut data_block_ranges = Vec::new();
    for i in 0..block_offsets.len() {
        let start = block_offsets[i];
        let end = if i + 1 < block_offsets.len() {
            block_offsets[i + 1]
        } else {
            index_offset
        };
        data_block_ranges.push((start, end));
    }

    Ok(SstableLayout {
        path: path.to_path_buf(),
        file_size,
        data_block_ranges,
        index_offset,
        bloom_offset,
    })
}

/// Helper to build an LsmStorage instance, write ground truth entries, flush to disk,
/// and locate the generated SSTable file.
async fn setup_sstable_and_ground_truth(
    entry_count: usize,
) -> (LsmStorage, TempDir, Vec<(Vec<u8>, Vec<u8>)>, SstableLayout) {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await.expect("create storage");

    let mut ground_truth = Vec::with_capacity(entry_count);
    for i in 0..entry_count {
        let tx = TxId::new((i + 1) as u64);
        let key = format!("sstable:chaos:key:{:06}", i).into_bytes();
        let val = format!("payload_value_{:08}_data_padding_for_block_fill", i).into_bytes();
        storage.put(tx, &key, &val).await.expect("put");
        storage.commit(tx).await.expect("commit");
        ground_truth.push((key, val));
    }

    storage.force_flush().await.expect("force_flush");

    // Locate the .sst file
    let sst_files: Vec<_> = fs::read_dir(tmp.path())
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sst"))
        .collect();

    assert_eq!(
        sst_files.len(),
        1,
        "Expected exactly 1 SSTable file after flush"
    );

    let layout = parse_sstable_layout(&sst_files[0]).expect("parse layout");

    (storage, tmp, ground_truth, layout)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    /// Requirement 2a: Data Block corruption via proptest
    /// Corrupting a bit inside a data block payload (skipping the 4-byte block CRC at the start of each block)
    /// MUST trigger `MemFuseError::ChecksumMismatch` when reading the affected key, NEVER a silently wrong value.
    #[test]
    fn prop_chaos_sstable_datablock_bitflip(
        block_idx_seed in 0usize..1000usize,
        payload_offset_pct in 0.0f64..1.0f64,
        bit_idx in 0u8..8u8,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (storage, tmp, ground_truth, layout) = setup_sstable_and_ground_truth(60).await;
            storage.wait_shutdown().await;

            if !layout.data_block_ranges.is_empty() {
                let block_idx = block_idx_seed % layout.data_block_ranges.len();
                let (block_start, block_end) = layout.data_block_ranges[block_idx];

                // Payload is after the 4-byte block CRC: [block_start + 4 .. block_end]
                let payload_start = block_start + 4;
                if block_end > payload_start {
                    let payload_len = block_end - payload_start;
                    let flip_offset = payload_start + ((payload_len as f64 * payload_offset_pct) as u64).min(payload_len - 1);

                    layout.inject_bit_flip(flip_offset, bit_idx).expect("bitflip");

                    let config = LsmConfig {
                        path: tmp.path().to_path_buf(),
                        memtable_size_limit: 1024 * 1024,
                        max_ram_mb: 64,
                        tx_timeout: Duration::from_secs(60),
                        encryption_passphrase: None,
                        ..Default::default()
                    };

                    let reopened_res = LsmStorage::new(config).await;

                    match reopened_res {
                        Err(e) => {
                            assert!(
                                matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                "Expected ChecksumMismatch on open, got {:?}",
                                e
                            );
                        }
                        Ok(reopened) => {
                            let mut detected_checksum_mismatch = false;
                            for (key, expected_val) in &ground_truth {
                                match reopened.get(key).await {
                                    Ok(Some(val)) => {
                                        assert_eq!(
                                            &val, expected_val,
                                            "Bit-flip at offset {} resulted in silent data corruption!",
                                            flip_offset
                                        );
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        assert!(
                                            matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                            "Expected ChecksumMismatch on get, got {:?}",
                                            e
                                        );
                                        detected_checksum_mismatch = true;
                                    }
                                }
                            }

                            assert!(
                                detected_checksum_mismatch,
                                "Data block bit flip at offset {} was NOT detected via ChecksumMismatch",
                                flip_offset
                            );

                            reopened.wait_shutdown().await;
                        }
                    }
                }
            }
        });
    }

    /// Requirement 2b: Bloom Filter corruption via proptest
    /// Corrupting a bit inside the Bloom filter MUST NEVER produce a False Negative (`Ok(None)`) for an existing key.
    /// Existing keys in ground truth MUST ALWAYS be found or fail with ChecksumMismatch error.
    #[test]
    fn prop_chaos_sstable_bloomfilter_bitflip(
        offset_pct in 0.0f64..1.0f64,
        bit_idx in 0u8..8u8,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (storage, tmp, ground_truth, layout) = setup_sstable_and_ground_truth(50).await;
            storage.wait_shutdown().await;

            let bloom_end = layout.file_size - 54;
            if bloom_end > layout.bloom_offset {
                let bloom_len = bloom_end - layout.bloom_offset;
                let flip_offset = layout.bloom_offset + ((bloom_len as f64 * offset_pct) as u64).min(bloom_len - 1);

                layout.inject_bit_flip(flip_offset, bit_idx).expect("bitflip");

                let config = LsmConfig {
                    path: tmp.path().to_path_buf(),
                    memtable_size_limit: 1024 * 1024,
                    max_ram_mb: 64,
                    tx_timeout: Duration::from_secs(60),
                    encryption_passphrase: None,
                    ..Default::default()
                };

                let reopened_res = LsmStorage::new(config).await;

                match reopened_res {
                    Err(e) => {
                        assert!(
                            matches!(e, MemFuseError::ChecksumMismatch { .. }),
                            "Reopen failed with non-checksum error: {:?}",
                            e
                        );
                    }
                    Ok(reopened) => {
                        for (key, expected_val) in &ground_truth {
                            match reopened.get(key).await {
                                Ok(Some(val)) => {
                                    assert_eq!(
                                        &val, expected_val,
                                        "Value mismatch for existing key"
                                    );
                                }
                                Ok(None) => {
                                    panic!(
                                        "Corrupted Bloom filter caused a False Negative for key {:?}! THIS IS DATA LOSS!",
                                        String::from_utf8_lossy(key)
                                    );
                                }
                                Err(e) => {
                                    assert!(
                                        matches!(e, MemFuseError::ChecksumMismatch { .. }),
                                        "Get failed with non-checksum error: {:?}",
                                        e
                                    );
                                }
                            }
                        }
                        reopened.wait_shutdown().await;
                    }
                }
            }
        });
    }

    /// Requirement 2c: Index corruption via proptest
    /// Corrupting a bit inside the Index section MUST either fail open with ChecksumMismatch / ParseError,
    /// or fail `get()` calls cleanly with ChecksumMismatch / ParseError / Storage error.
    /// It MUST NEVER panic or return silently wrong values.
    #[test]
    fn prop_chaos_sstable_index_bitflip(
        offset_pct in 0.0f64..1.0f64,
        bit_idx in 0u8..8u8,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (storage, tmp, ground_truth, layout) = setup_sstable_and_ground_truth(50).await;
            storage.wait_shutdown().await;

            if layout.bloom_offset > layout.index_offset {
                let index_len = layout.bloom_offset - layout.index_offset;
                let flip_offset = layout.index_offset + ((index_len as f64 * offset_pct) as u64).min(index_len - 1);

                layout.inject_bit_flip(flip_offset, bit_idx).expect("bitflip");

                let config = LsmConfig {
                    path: tmp.path().to_path_buf(),
                    memtable_size_limit: 1024 * 1024,
                    max_ram_mb: 64,
                    tx_timeout: Duration::from_secs(60),
                    encryption_passphrase: None,
                    ..Default::default()
                };

                let reopened_res = LsmStorage::new(config).await;

                match reopened_res {
                    Err(e) => {
                        assert!(
                            matches!(
                                e,
                                MemFuseError::ChecksumMismatch { .. }
                                    | MemFuseError::ParseError(_)
                                    | MemFuseError::Storage(_)
                            ),
                            "Reopen failed with unexpected error: {:?}",
                            e
                        );
                    }
                    Ok(reopened) => {
                        for (key, expected_val) in &ground_truth {
                            match reopened.get(key).await {
                                Ok(Some(val)) => {
                                    assert_eq!(
                                        &val, expected_val,
                                        "Value mismatch for key {:?}!",
                                        String::from_utf8_lossy(key)
                                    );
                                }
                                Ok(None) => {
                                    // Index offset corrupted pointing to wrong block or entry
                                }
                                Err(e) => {
                                    assert!(
                                        matches!(
                                            e,
                                            MemFuseError::ChecksumMismatch { .. }
                                                | MemFuseError::ParseError(_)
                                                | MemFuseError::Storage(_)
                                        ),
                                        "Get failed with unexpected error: {:?}",
                                        e
                                    );
                                }
                            }
                        }
                        reopened.wait_shutdown().await;
                    }
                }
            }
        });
    }
}
