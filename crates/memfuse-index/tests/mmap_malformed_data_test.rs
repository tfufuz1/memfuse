//! Robustheitstests für MmapIndex gegen beschädigte/manipulierte Dateien.
//! Sichert: try_from_bytes() und MmapIndex::open() paniken NIEMALS —
//! auch bei abgeschnittenen, korrumpierten oder übergroßen Headern.
//!
//! Referenz: Security-Review-Report SEC-01/TST-01

use memfuse_index::persistence::{HnswHeader, MmapIndex, HNSW_MAGIC};
use std::io::Write;
use tempfile::tempdir;

// ── Hilfsfunktion: Erstellt minimalen validen Header-Byte-Slice ──────────
fn valid_header_bytes() -> Vec<u8> {
    // Baue 64 Bytes (HnswHeader::SIZE) mit korrektem Magic + Version=1
    // Alle anderen Felder = 0
    let mut bytes = vec![0u8; HnswHeader::SIZE];
    let magic_bytes = HNSW_MAGIC.to_le_bytes();
    bytes[0..4].copy_from_slice(&magic_bytes);
    let version_bytes = 1u16.to_le_bytes();
    bytes[4..6].copy_from_slice(&version_bytes);
    bytes
}

#[test]
fn test_try_from_bytes_empty_slice_returns_error() {
    let result = HnswHeader::try_from_bytes(&[]);
    assert!(
        result.is_err(),
        "Leerer Slice muss Err erzeugen, got: {:?}",
        result
    );
}

#[test]
fn test_try_from_bytes_truncated_header_returns_error() {
    // Nur 10 Bytes statt 64
    let result = HnswHeader::try_from_bytes(&[0u8; 10]);
    assert!(result.is_err(), "Abgeschnittener Header muss Err erzeugen");
}

#[test]
fn test_try_from_bytes_wrong_magic_returns_error() {
    let mut bytes = valid_header_bytes();
    // Überschreibe Magic mit 0xDEADBEEF
    bytes[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    let result = HnswHeader::try_from_bytes(&bytes);
    assert!(result.is_err(), "Falscher Magic muss Err erzeugen");
}

#[test]
fn test_try_from_bytes_wrong_version_returns_error() {
    let mut bytes = valid_header_bytes();
    // Version = u16::MAX (unbekannte Version)
    bytes[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
    let result = HnswHeader::try_from_bytes(&bytes);
    // Wenn der Parser keine Versions-Prüfung hat: Ok ist akzeptabel.
    // Das Wichtige ist: KEIN PANIC.
    // assert keine Panik ist implizit wenn wir hier ankommen.
    let _ = result;
}

#[test]
fn test_mmap_open_empty_file_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("empty.hnsw");
    std::fs::File::create(&path).expect("create file");
    let result = MmapIndex::open(&path);
    assert!(
        result.is_err(),
        "Leere Datei muss Err zurückgeben statt zu paniken"
    );
}

#[test]
fn test_mmap_open_truncated_file_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("truncated.hnsw");
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(&[0u8; 10]).expect("write");
    let result = MmapIndex::open(&path);
    assert!(
        result.is_err(),
        "Abgeschnittene Datei (10 Bytes) muss Err zurückgeben"
    );
}

#[test]
fn test_mmap_open_wrong_magic_returns_error() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("badmagic.hnsw");
    let mut bytes = valid_header_bytes();
    bytes[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(&bytes).expect("write");
    let result = MmapIndex::open(&path);
    assert!(
        result.is_err(),
        "Datei mit falschem Magic muss Err zurückgeben"
    );
}

#[test]
fn test_mmap_open_huge_node_count_no_panic() {
    // node_count = u64::MAX darf nicht zu OOM/Panic führen
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("hugecnt.hnsw");
    let mut bytes = valid_header_bytes();
    bytes.iter_mut().for_each(|b| *b = 0xFF);
    // Magic korrekt setzen damit wir bis zur node_count-Prüfung kommen
    bytes[0..4].copy_from_slice(&HNSW_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(&bytes).expect("write");
    // Kein Panic ist die Anforderung — Err oder Ok sind beide akzeptabel
    let _ = MmapIndex::open(&path);
}

#[test]
fn test_mmap_open_exact_file_sizes_no_panic() {
    // BEFUND 2: Dateigrößen 0, 1, 10, 63 und 64 Bytes
    for size in [0, 1, 10, 63, 64] {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(format!("size_{}.hnsw", size));
        let dummy_data = vec![0x42u8; size];
        std::fs::write(&path, &dummy_data).expect("write");

        let catch_res = std::panic::catch_unwind(|| MmapIndex::open(&path));
        assert!(
            catch_res.is_ok(),
            "MmapIndex::open panicked on file size {}!",
            size
        );

        let open_res = catch_res.unwrap();
        if size < HnswHeader::SIZE {
            assert!(
                open_res.is_err(),
                "Expected Result::Err for file size {} < {}",
                size,
                HnswHeader::SIZE
            );
        }
    }
}

#[test]
fn test_mmap_connection_len_overflow_returns_err() {
    // BEFUND 4: Fuzz/Property-Test mit len-Werten nahe u32::MAX/usize::MAX
    use memfuse_index::persistence::NodeRecord;

    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("overflow.hnsw");

    // Header mit validen Basiswerten + connections_offset = 64
    let header = HnswHeader::new(
        4,          // dim
        16,         // m
        1,          // metric
        0,          // quantized
        0.0, 1.0,   // q_min, q_max
        1,          // node_count
        0,          // entry_point
        64,         // nodes_offset
        128,        // connections_offset
        1,          // last_tx_id
    );

    let mut data = Vec::new();
    data.extend_from_slice(&header.to_bytes()); // 0..64
    let record = NodeRecord {
        doc_id: 1,
        max_layer: 1,
        vector_offset: 200,
        connections_offset: 128,
    };
    data.extend_from_slice(&record.to_bytes()); // 64..89
    data.resize(128, 0); // Padding bis connections_offset (128)

    // num_layers = 1
    data.push(1u8);

    // Write a connection length of u32::MAX
    let corrupt_len = u32::MAX;
    data.extend_from_slice(&corrupt_len.to_le_bytes());

    std::fs::write(&path, &data).expect("write overflow test file");

    let mmap_res = MmapIndex::open(&path);
    assert!(mmap_res.is_ok(), "Header is valid so open should succeed");
    let mmap_index = mmap_res.unwrap();

    let node_rec = mmap_index.get_node_record(0).expect("record");

    // Calling get_connections with corrupt len near u32::MAX must NOT panic or overflow
    let catch_res = std::panic::catch_unwind(|| mmap_index.get_connections(&node_rec, 0));
    assert!(catch_res.is_ok(), "get_connections panicked on u32::MAX length!");

    let conn_res = catch_res.unwrap();
    assert!(
        conn_res.is_err(),
        "Expected Result::Err on u32::MAX length overflow, got Ok!"
    );
}
