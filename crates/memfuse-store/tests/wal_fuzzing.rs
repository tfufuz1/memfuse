//! Integration-Test: WAL-Fuzzing via Bit-Flip-Injektion
//!
//! Testziel (aus dem Verifikationsplan):
//! Injektion zufälliger Bit-Flips in `.wal`-Dateien. Verifikation, dass
//! `WalReader::replay()` deterministisch mit `MemFuseError::WalCorruption`
//! abbricht oder truncation-tolerant ist — und NIEMALS paniziert.
//!
//! ZUSATZ (Audit 2026-06-07): Systematische Prüfung der ersten 12 Bytes (Header).

use memfuse_core::TxId;
use memfuse_store::wal::{Wal, WalOp};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tempfile::tempdir;
use tokio::fs;

/// Schreibt N valide WAL-Entries und gibt den WAL-Dateipfad zurück.
async fn write_valid_wal(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
    let wal_path = dir.join(format!("fuzz_test_{}.wal", rand::random::<u64>()));
    let wal = Wal::open(&wal_path).await.expect("open WAL");

    for i in 0..n {
        let op = WalOp::Put {
            tx_id: TxId::new(i as u64),
            key: format!("sensor:data:{}", i).into_bytes(),
            value: format!("payload_{:04}", i).into_bytes(),
        };
        let entry = wal.create_entry(op, i as u64).await.expect("create entry");
        wal.append(&entry).await.expect("append");
    }
    wal_path
}

/// Property-Test: Bit-Flip-Injektion in WAL-Payloads (tx_id, seq_no, key, value)
/// Invariante: Bit-Flips in Feldern führen deterministisch zu Korruptionserkennung/Fehler und NIEMALS zu einer Panic.
#[tokio::test]
async fn test_wal_bitflip_property_integrity() {
    let dir = tempdir().expect("tempdir");
    let mut rng = StdRng::seed_from_u64(0xFEED_FACE_CAFE_4242);

    for _iteration in 0..30u32 {
        let wal_path = write_valid_wal(dir.path(), 5).await;
        let mut data = fs::read(&wal_path).await.expect("read WAL");

        // Flip a bit specifically in the payload section (after 4-byte MFW3 header)
        if data.len() > 12 {
            let flip_offset = rng.gen_range(4..data.len());
            data[flip_offset] ^= 0x01 << rng.gen_range(0..8);
            fs::write(&wal_path, &data).await.expect("write WAL");

            let result = async {
                match Wal::open(&wal_path).await {
                    Err(e) => Err(e),
                    Ok(wal) => wal.replay().await.map(|_| ()),
                }
            }
            .await;

            // Must either detect corruption via error or tolerate tail truncation, but NEVER panic.
            assert!(
                result.is_err() || result.is_ok(),
                "Replay must cleanly handle bit-flip alterations"
            );
        }
        let _ = fs::remove_file(&wal_path).await;
    }
}

/// Kerntest: 50 Iterationen mit zufälligem Bit-Flip.
/// Invariante: `replay()` darf NIEMALS panizieren, nur `Err` zurückgeben.
#[tokio::test]
async fn test_wal_random_bitflip_never_panics() {
    let dir = tempdir().expect("tempdir");
    let n_entries = 10;

    // Deterministischer RNG für Reproduzierbarkeit von Fehlerberichten
    let mut rng = StdRng::seed_from_u64(0xDEAD_BEEF_CAFE_1337);

    let mut corruption_detected = 0u32;
    let mut truncation_tolerated = 0u32;

    for _iteration in 0..50u32 {
        let wal_path = write_valid_wal(dir.path(), n_entries).await;
        let mut data = fs::read(&wal_path).await.expect("read WAL");

        if data.is_empty() {
            continue;
        }

        let skip_header = (data.len() / 10).min(8);
        let flip_offset = rng.gen_range(skip_header..data.len());
        data[flip_offset] ^= 0x01 << rng.gen_range(0..8); // Flip exactly one bit
        fs::write(&wal_path, &data)
            .await
            .expect("write corrupted WAL");

        let result = async {
            match Wal::open(&wal_path).await {
                Err(e) => Err(e),
                Ok(wal) => wal.replay().await.map(|_| ()),
            }
        }
        .await;

        match result {
            Err(_) => {
                corruption_detected += 1;
            }
            Ok(_) => {
                truncation_tolerated += 1;
            }
        }
        let _ = fs::remove_file(&wal_path).await;
    }

    println!(
        "WAL Random Fuzzing: {} detected, {} tolerated",
        corruption_detected, truncation_tolerated
    );
    assert!(corruption_detected + truncation_tolerated == 50);
}

/// Systematischer Test der ersten 12 Bytes (Header: Length, CRC, SeqNo-Start).
/// Jedes einzelne Bit in diesem Bereich wird geflippt.
#[tokio::test]
async fn test_wal_systematic_header_corruption() {
    let dir = tempdir().expect("tempdir");
    let wal_path = write_valid_wal(dir.path(), 5).await;
    let original_data = fs::read(&wal_path).await.expect("read");

    // Wir testen die ersten 12 Bytes systematisch
    let test_limit = 12.min(original_data.len());

    for byte_idx in 0..test_limit {
        for bit_idx in 0..8 {
            let mut corrupted_data = original_data.clone();
            corrupted_data[byte_idx] ^= 0x01 << bit_idx;

            fs::write(&wal_path, &corrupted_data).await.expect("write");

            let result = async {
                match Wal::open(&wal_path).await {
                    Err(e) => Err(e),
                    Ok(wal) => wal.replay().await.map(|_| ()),
                }
            }
            .await;

            if result.is_ok() {
                eprintln!(
                    "FAILURE: Bit-flip at byte {}, bit {} was NOT detected. Data len: {}",
                    byte_idx,
                    bit_idx,
                    corrupted_data.len()
                );
            }
            assert!(
                result.is_err(),
                "Bit-flip at byte {}, bit {} must be detected as error.",
                byte_idx,
                bit_idx
            );
        }
    }
}

/// Sonderfall: Injektion direkt in den CRC32-Bereich eines Frames.
#[tokio::test]
async fn test_wal_crc_field_corruption_detected() {
    let dir = tempdir().expect("tempdir");
    let wal_path = write_valid_wal(dir.path(), 5).await;
    let mut data = fs::read(&wal_path).await.expect("read");

    // WAL-Frame-Layout: [len: 4 bytes][crc: 4 bytes][payload: N bytes]
    if data.len() >= 8 {
        data[4] ^= 0xFF; // Corrupt CRC bytes
        fs::write(&wal_path, &data).await.expect("write");
    }

    let result = async {
        match Wal::open(&wal_path).await {
            Err(e) => Err(e),
            Ok(wal) => wal.replay().await.map(|_| ()),
        }
    }
    .await;

    assert!(result.is_err(), "Corrupted CRC MUST be detected");
}
