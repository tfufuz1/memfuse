//! Integration tests for WAL HMAC Binding and Tamper Attack Scenarios (Round 2 Audit).
//!
//! Evaluates the 4 attack scenarios specified in the audit assignment:
//! a) Swap Block 2 and Block 4 (Reordering)
//! b) Remove Block 5 completely (Tail Truncation)
//! c) Duplicate Block 3 within the sequence (In-file Replay)
//! d) Extract Block 3 from WAL A and insert into WAL B (Cross-file Replay)

use memfuse_core::{MemFuseError, TxId};
use memfuse_crypto::crypto::KeyManager;
use memfuse_store::wal::{Wal, WalOp, WAL_V3_HEADER};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::fs;

/// Helper to create 5 distinct WAL entries individually appended.
async fn create_5_entry_wal_unencrypted(
    dir: &std::path::Path,
) -> (std::path::PathBuf, Vec<[u8; 32]>) {
    let wal_path = dir.join("test_5_entries.wal");
    let wal = Wal::open(&wal_path).await.expect("open WAL");

    let mut hmacs = Vec::new();

    for i in 1..=5 {
        let op = WalOp::Put {
            tx_id: TxId::new(i),
            key: format!("key_{i}").into_bytes(),
            value: format!("val_{i}").into_bytes(),
        };
        let entry = wal.create_entry(op, i).await.expect("create entry");
        wal.append(&entry).await.expect("append entry");
        hmacs.push(entry.checksum);
    }

    (wal_path, hmacs)
}

/// Helper to parse individual unencrypted WAL entry raw bytes from a WAL file.
async fn read_unencrypted_raw_entries(path: &std::path::Path) -> Vec<Vec<u8>> {
    let data = fs::read(path).await.expect("read file");
    let mut offset = 0;
    if data.starts_with(&WAL_V3_HEADER) {
        offset = 4;
    }

    let mut entry_chunks = Vec::new();
    while offset < data.len() {
        if offset + 4 > data.len() {
            break;
        }
        let len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let total_chunk_len = 4 + len;
        if offset + total_chunk_len > data.len() {
            break;
        }
        entry_chunks.push(data[offset..offset + total_chunk_len].to_vec());
        offset += total_chunk_len;
    }
    entry_chunks
}

/// Helper to parse encrypted WAL chunks (header + len + nonce + ciphertext) from a WAL file.
async fn read_encrypted_raw_chunks(path: &std::path::Path) -> Vec<Vec<u8>> {
    let data = fs::read(path).await.expect("read file");
    let mut offset = 0;
    if data.starts_with(&WAL_V3_HEADER) {
        offset = 4;
    }

    let mut chunks = Vec::new();
    while offset < data.len() {
        if offset + 4 > data.len() {
            break;
        }
        let len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let total_chunk_len = 4 + len;
        if offset + total_chunk_len > data.len() {
            break;
        }
        chunks.push(data[offset..offset + total_chunk_len].to_vec());
        offset += total_chunk_len;
    }
    chunks
}

// =========================================================================
// ATTACK A: Swap Block 2 and Block 4 (Reordering)
// =========================================================================

#[tokio::test]
async fn test_attack_a_swap_blocks_unencrypted_detected() {
    let dir = tempdir().expect("tempdir");
    let (wal_path, _) = create_5_entry_wal_unencrypted(dir.path()).await;

    let chunks = read_unencrypted_raw_entries(&wal_path).await;
    assert_eq!(chunks.len(), 5, "Expected 5 raw entry chunks");

    // Construct tampered file: Header + Chunk 0 + Chunk 3 (Block 4) + Chunk 2 + Chunk 1 (Block 2) + Chunk 4
    let mut tampered_data = Vec::new();
    tampered_data.extend_from_slice(&WAL_V3_HEADER);
    tampered_data.extend_from_slice(&chunks[0]); // Block 1
    tampered_data.extend_from_slice(&chunks[3]); // Block 4 swapped into pos 2
    tampered_data.extend_from_slice(&chunks[2]); // Block 3
    tampered_data.extend_from_slice(&chunks[1]); // Block 2 swapped into pos 4
    tampered_data.extend_from_slice(&chunks[4]); // Block 5

    fs::write(&wal_path, &tampered_data)
        .await
        .expect("write tampered file");

    let open_res = Wal::open(&wal_path).await;

    assert!(
        open_res.is_err(),
        "Attack a (swap blocks) MUST be detected during open/recovery"
    );
    let err = open_res.unwrap_err();
    assert!(
        matches!(err, MemFuseError::WalCorruption { .. }),
        "Expected WalCorruption error, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_attack_a_swap_blocks_encrypted_detected() {
    let dir = tempdir().expect("tempdir");
    let wal_path = dir.path().join("encrypted_swap.wal");
    let km =
        Arc::new(KeyManager::try_new("pass_swap", b"salt123456789012345678901234567890").unwrap());

    {
        let wal = Wal::open_with_key_manager(&wal_path, Some(km.clone()))
            .await
            .expect("open wal");
        for i in 1..=5 {
            let op = WalOp::Put {
                tx_id: TxId::new(i),
                key: format!("k{i}").into_bytes(),
                value: format!("v{i}").into_bytes(),
            };
            let entry = wal.create_entry(op, i).await.expect("entry");
            wal.append(&entry).await.expect("append");
        }
    }

    let chunks = read_encrypted_raw_chunks(&wal_path).await;
    assert_eq!(chunks.len(), 5);

    // Swap chunk 1 (Block 2) and chunk 3 (Block 4)
    let mut tampered_data = Vec::new();
    tampered_data.extend_from_slice(&WAL_V3_HEADER);
    tampered_data.extend_from_slice(&chunks[0]);
    tampered_data.extend_from_slice(&chunks[3]); // Block 4
    tampered_data.extend_from_slice(&chunks[2]);
    tampered_data.extend_from_slice(&chunks[1]); // Block 2
    tampered_data.extend_from_slice(&chunks[4]);

    fs::write(&wal_path, &tampered_data).await.expect("write");

    let open_res = Wal::open_with_key_manager(&wal_path, Some(km)).await;

    assert!(
        open_res.is_err(),
        "Encrypted Attack a (swap blocks) MUST be detected during open/recovery"
    );
    assert!(matches!(
        open_res.unwrap_err(),
        MemFuseError::WalCorruption { .. }
    ));
}

// =========================================================================
// ATTACK B: Truncation (Remove Block 5 completely)
// =========================================================================

#[tokio::test]
async fn test_attack_b_truncation_last_block_behavior() {
    let dir = tempdir().expect("tempdir");
    let (wal_path, _) = create_5_entry_wal_unencrypted(dir.path()).await;

    let chunks = read_unencrypted_raw_entries(&wal_path).await;
    assert_eq!(chunks.len(), 5);

    // Remove Block 5 completely by truncating file to contain only header + chunks 0..4
    let mut truncated_data = Vec::new();
    truncated_data.extend_from_slice(&WAL_V3_HEADER);
    for chunk in &chunks[0..4] {
        truncated_data.extend_from_slice(chunk);
    }

    fs::write(&wal_path, &truncated_data).await.expect("write");

    let wal = Wal::open(&wal_path).await.expect("open wal");
    let res = wal.replay().await;

    // Document exact behavior:
    // Tail truncation at a clean entry boundary leaves the remaining prefix (Blocks 1..4)
    // with valid HMAC chain links. `replay()` returns 4 entries successfully without an error.
    assert!(
        res.is_ok(),
        "Replay of clean tail truncation succeeds for prefix entries"
    );
    let entries = res.unwrap();
    assert_eq!(
        entries.len(),
        4,
        "Block 5 was truncated; replay returns first 4 entries without error"
    );
}

// =========================================================================
// ATTACK C: Duplication / In-File Replay (Duplicate Block 3)
// =========================================================================

#[tokio::test]
async fn test_attack_c_duplicate_block_3_unencrypted_detected() {
    let dir = tempdir().expect("tempdir");
    let (wal_path, _) = create_5_entry_wal_unencrypted(dir.path()).await;

    let chunks = read_unencrypted_raw_entries(&wal_path).await;
    assert_eq!(chunks.len(), 5);

    // Duplicate Block 3: sequence becomes 1, 2, 3, 3, 4, 5
    let mut tampered_data = Vec::new();
    tampered_data.extend_from_slice(&WAL_V3_HEADER);
    tampered_data.extend_from_slice(&chunks[0]); // Block 1
    tampered_data.extend_from_slice(&chunks[1]); // Block 2
    tampered_data.extend_from_slice(&chunks[2]); // Block 3
    tampered_data.extend_from_slice(&chunks[2]); // Duplicated Block 3
    tampered_data.extend_from_slice(&chunks[3]); // Block 4
    tampered_data.extend_from_slice(&chunks[4]); // Block 5

    fs::write(&wal_path, &tampered_data).await.expect("write");

    let open_res = Wal::open(&wal_path).await;

    assert!(
        open_res.is_err(),
        "Attack c (duplicate block) MUST be detected during open/recovery"
    );
    let err = open_res.unwrap_err();
    assert!(
        matches!(err, MemFuseError::WalCorruption { .. }),
        "Expected WalCorruption error, got: {:?}",
        err
    );
}

// =========================================================================
// ATTACK D: Cross-File Replay (Insert Block 3 from WAL A into WAL B)
// =========================================================================

#[tokio::test]
async fn test_attack_d_cross_file_replay_unencrypted_detected() {
    let dir1 = tempdir().expect("dir1");
    let dir2 = tempdir().expect("dir2");

    let (wal_path_a, _) = create_5_entry_wal_unencrypted(dir1.path()).await;
    let (wal_path_b, _) = create_5_entry_wal_unencrypted(dir2.path()).await;

    let chunks_a = read_unencrypted_raw_entries(&wal_path_a).await;
    let chunks_b = read_unencrypted_raw_entries(&wal_path_b).await;

    // Inject Block 3 from WAL A into WAL B at position 3:
    // WAL B becomes: WAL B Block 1, WAL B Block 2, WAL A Block 3, WAL B Block 4, WAL B Block 5
    let mut tampered_data = Vec::new();
    tampered_data.extend_from_slice(&WAL_V3_HEADER);
    tampered_data.extend_from_slice(&chunks_b[0]); // B1
    tampered_data.extend_from_slice(&chunks_b[1]); // B2
    tampered_data.extend_from_slice(&chunks_a[2]); // A3 injected!
    tampered_data.extend_from_slice(&chunks_b[3]); // B4
    tampered_data.extend_from_slice(&chunks_b[4]); // B5

    fs::write(&wal_path_b, &tampered_data).await.expect("write");

    let open_res = Wal::open(&wal_path_b).await;

    assert!(
        open_res.is_err(),
        "Attack d (cross-file replay unencrypted) MUST be detected during open/recovery"
    );
    let err = open_res.unwrap_err();
    assert!(
        matches!(err, MemFuseError::WalCorruption { .. }),
        "Expected WalCorruption error, got: {:?}",
        err
    );
}

#[tokio::test]
async fn test_attack_d_cross_file_replay_encrypted_detected() {
    let dir1 = tempdir().expect("dir1");
    let dir2 = tempdir().expect("dir2");

    let wal_path_a = dir1.path().join("wal_a.wal");
    let wal_path_b = dir2.path().join("wal_b.wal");

    let km =
        Arc::new(KeyManager::try_new("cross_pass", b"salt123456789012345678901234567890").unwrap());

    // Create WAL A
    {
        let wal_a = Wal::open_with_key_manager(&wal_path_a, Some(km.clone()))
            .await
            .unwrap();
        for i in 1..=5 {
            let op = WalOp::Put {
                tx_id: TxId::new(i),
                key: format!("a_k{i}").into_bytes(),
                value: format!("a_v{i}").into_bytes(),
            };
            let entry = wal_a.create_entry(op, i).await.unwrap();
            wal_a.append(&entry).await.unwrap();
        }
    }

    // Create WAL B
    {
        let wal_b = Wal::open_with_key_manager(&wal_path_b, Some(km.clone()))
            .await
            .unwrap();
        for i in 1..=5 {
            let op = WalOp::Put {
                tx_id: TxId::new(i),
                key: format!("b_k{i}").into_bytes(),
                value: format!("b_v{i}").into_bytes(),
            };
            let entry = wal_b.create_entry(op, i).await.unwrap();
            wal_b.append(&entry).await.unwrap();
        }
    }

    let chunks_a = read_encrypted_raw_chunks(&wal_path_a).await;
    let chunks_b = read_encrypted_raw_chunks(&wal_path_b).await;

    // Inject WAL A Block 3 ciphertext chunk into WAL B
    let mut tampered_data = Vec::new();
    tampered_data.extend_from_slice(&WAL_V3_HEADER);
    tampered_data.extend_from_slice(&chunks_b[0]);
    tampered_data.extend_from_slice(&chunks_b[1]);
    tampered_data.extend_from_slice(&chunks_a[2]); // A3 injected into B
    tampered_data.extend_from_slice(&chunks_b[3]);
    tampered_data.extend_from_slice(&chunks_b[4]);

    fs::write(&wal_path_b, &tampered_data).await.expect("write");

    let open_res = Wal::open_with_key_manager(&wal_path_b, Some(km)).await;

    assert!(
        open_res.is_err(),
        "Attack d (cross-file encrypted replay) MUST be detected during open/recovery"
    );
    let err = open_res.unwrap_err();
    assert!(
        matches!(err, MemFuseError::WalCorruption { .. }),
        "Expected WalCorruption error due to per-file UUID key isolation failure, got: {:?}",
        err
    );
}
