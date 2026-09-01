//! Robustheitstests für MmapIndex und DiskAnnIndex gegen beschädigte Dateien.
//! Jeder Test schreibt eine präparierte Datei in ein tempdir() und prüft,
//! dass der Parse-Pfad Err zurückgibt statt zu paniken.

use memfuse_core::MemFuseError;
use memfuse_index::persistence::{HnswHeader, MmapIndex, HNSW_MAGIC};
use std::io::Write;
use tempfile::tempdir;

// Test 1: Leere Datei → MemFuseError::Storage
#[test]
fn test_mmap_open_empty_file_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("empty.hnsw");
    std::fs::File::create(&path).expect("create file");

    let res = MmapIndex::open(&path);
    assert!(matches!(res, Err(MemFuseError::Storage(_))));
}

// Test 2: Datei kürzer als HnswHeader::SIZE → MemFuseError::Storage
#[test]
fn test_mmap_open_truncated_header_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("truncated.hnsw");
    let mut file = std::fs::File::create(&path).expect("create file");
    file.write_all(&[0u8; 10]).expect("write truncated header");
    file.sync_all().expect("sync");

    let res = MmapIndex::open(&path);
    assert!(matches!(res, Err(MemFuseError::Storage(_))));
}

// Test 3: Falscher Magic-Wert (0xDEADBEEF statt HNSW_MAGIC) → MemFuseError::Storage
#[test]
fn test_mmap_open_wrong_magic_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("bad_magic.hnsw");
    let mut file = std::fs::File::create(&path).expect("create file");
    let mut bytes = [0u8; HnswHeader::SIZE];
    bytes[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    file.write_all(&bytes).expect("write bad magic header");
    file.sync_all().expect("sync");

    let res = MmapIndex::open(&path);
    assert!(matches!(res, Err(MemFuseError::Storage(_))));
}

// Test 4: Korrekter Magic, aber node_count = u64::MAX → kein Panic
#[test]
fn test_mmap_open_huge_node_count_no_panic() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("huge_node_count.hnsw");
    let mut file = std::fs::File::create(&path).expect("create file");
    let mut bytes = [0u8; HnswHeader::SIZE];
    bytes[0..4].copy_from_slice(&HNSW_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes()); // version
    bytes[24..32].copy_from_slice(&u64::MAX.to_le_bytes()); // node_count
    file.write_all(&bytes).expect("write header");
    file.sync_all().expect("sync");

    let res = MmapIndex::open(&path);
    // Soll keinesfalls paniken, unabhängig davon ob ok oder err zurückkommt
    let _ = res;
    assert!(true);
}

// Test 5: connections_offset zeigt hinter Dateiende → kein Panic
#[test]
fn test_mmap_open_connections_offset_beyond_eof_no_panic() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("bad_connections_offset.hnsw");
    let mut file = std::fs::File::create(&path).expect("create file");
    let mut bytes = [0u8; HnswHeader::SIZE];
    bytes[0..4].copy_from_slice(&HNSW_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[48..56].copy_from_slice(&u64::MAX.to_le_bytes()); // connections_offset
    file.write_all(&bytes).expect("write header");
    file.sync_all().expect("sync");

    let res = MmapIndex::open(&path);
    if let Ok(mmap_index) = res {
        // Falls MmapIndex::open klappt, sollte get_connections oder get_node_record nicht paniken
        let record = memfuse_index::persistence::NodeRecord {
            doc_id: 1,
            max_layer: 1,
            vector_offset: 64,
            connections_offset: u64::MAX,
        };
        let _ = mmap_index.get_connections(&record, 0);
    }
    assert!(true);
}

// Test 6: DiskANN — Datei kürzer als DiskAnnHeader → MemFuseError::Storage
#[cfg(feature = "experimental-diskann")]
#[tokio::test]
async fn test_diskann_load_truncated_header_returns_error() {
    use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};

    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("truncated.diskann");
    let mut file = std::fs::File::create(&path).expect("create file");
    file.write_all(&[0u8; 5]).expect("write truncated header");
    file.sync_all().expect("sync");

    let config = DiskAnnConfig {
        index_path: path,
        ..DiskAnnConfig::default()
    };
    let index = DiskAnnIndex::try_new(config).expect("valid config");
    let res = index.load().await;
    assert!(matches!(res, Err(MemFuseError::Storage(_))));
}

// Test 7: DiskANN — falscher Magic → MemFuseError::Storage
#[cfg(feature = "experimental-diskann")]
#[tokio::test]
async fn test_diskann_load_wrong_magic_returns_error() {
    use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};

    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("bad_magic.diskann");
    let mut file = std::fs::File::create(&path).expect("create file");
    let mut bytes = [0u8; 64];
    bytes[0..4].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
    file.write_all(&bytes).expect("write header");
    file.sync_all().expect("sync");

    let config = DiskAnnConfig {
        index_path: path,
        ..DiskAnnConfig::default()
    };
    let index = DiskAnnIndex::try_new(config).expect("valid config");
    let res = index.load().await;
    assert!(matches!(res, Err(MemFuseError::Storage(_))));
}
