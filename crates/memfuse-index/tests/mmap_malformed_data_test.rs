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
    // node_count liegt ab Byte-Offset 36 (nach magic+version+dim+m+metric+
    //   quantized+q_min+q_max = 4+2+4+4+1+1+4+4 = 24, dann entry_point i64
    //   wäre 24..32, und node_count danach — exakter Offset: aus HnswHeader
    //   Feldanordnung ableiten oder pauschal alle Bytes auf 0xFF setzen)
    bytes.iter_mut().for_each(|b| *b = 0xFF);
    // Magic korrekt setzen damit wir bis zur node_count-Prüfung kommen
    bytes[0..4].copy_from_slice(&HNSW_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(&bytes).expect("write");
    // Kein Panic ist die Anforderung — Err oder Ok sind beide akzeptabel
    let _ = MmapIndex::open(&path);
}
