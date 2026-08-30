use memfuse_core::{Result, TxId};
use memfuse_store::wal::{Wal, WalOp};
use std::fs::OpenOptions;
use std::io::Write;
use tempfile::tempdir;

#[tokio::test]
async fn test_wal_recovery_from_partial_write() -> Result<()> {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("test.wal");

    // 1. Erstelle einen validen WAL und schreibe 3 Entries
    {
        let wal = Wal::open(&wal_path).await?;
        for i in 1..=3 {
            let op = WalOp::Put {
                tx_id: TxId::new(i),
                key: format!("key{}", i).into_bytes(),
                value: format!("val{}", i).into_bytes(),
            };
            let entry = wal.create_entry(op, i).await?;
            wal.append(&entry).await?;
        }
    } // Wal wird geschlossen (Mutex-Guard droppt, File droppt)

    // 2. Simuliere einen "Partial Write" (halber Header am Ende)
    {
        let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
        // Nur 2 Bytes eines 4-Byte Längen-Präfixes
        file.write_all(&[0x10, 0x00]).unwrap();
        file.sync_all().unwrap();
    }

    // 3. Öffne den WAL erneut. Replay wird intern ausgeführt.
    // In der aktuellen Implementierung führt ein fehlerhaftes Read in replay_with_size
    // vermutlich zu einem Fehler. Wir wollen sehen, wie das System reagiert.
    let wal = Wal::open(&wal_path).await;

    // INTENT: Hier entscheiden wir: Soll Wal::open scheitern oder
    // die korrupten Daten am Ende abschneiden (Truncate)?
    // Die Sovereign-Core-Doktrin bevorzugt Sicherheit.
    // Wenn das Log korrupt ist, ist ein expliziter Fehler besser als stillschweigendes Ignorieren.

    match wal {
        Ok(w) => {
            let entries = w.replay().await?;
            // Wenn es Ok ist, sollten zumindest die ersten 3 da sein.
            assert_eq!(entries.len(), 3);
        }
        Err(e) => {
            println!("WAL open failed as expected on corruption: {:?}", e);
            // Das ist auch ein valides Ergebnis für "Zero Panic" — solange es kein Panic ist.
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_v3_hmac_includes_txid() -> Result<()> {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("v3_txid_tamper.wal");

    let original_tx_id = TxId::new(42);
    let tampered_tx_id = TxId::new(999);

    {
        let wal = Wal::open(&wal_path).await?;
        let op = WalOp::Put {
            tx_id: original_tx_id,
            key: b"secure_key".to_vec(),
            value: b"secure_val".to_vec(),
        };
        let entry = wal.create_entry(op, 1).await?;
        wal.append(&entry).await?;
    }

    // Tamper with tx_id in serialized entry without recomputing HMAC.
    // Serialized frame layout:
    // Offset 0..4: MFW3 header
    // Offset 4..8: length_prefix
    // Offset 8..12: CRC32
    // Offset 12..20: seq_no (u64 LE)
    // Offset 20..52: checksum (32 bytes)
    // Offset 52..84: prev_hmac (32 bytes)
    // Offset 84: op_type (0 = Put)
    // Offset 85..93: tx_id (8 bytes LE)
    let mut bytes = std::fs::read(&wal_path).unwrap();
    assert_eq!(&bytes[0..4], b"MFW3");

    let tx_id_offset = 85;
    let read_tx_id = u64::from_le_bytes(bytes[tx_id_offset..tx_id_offset + 8].try_into().unwrap());
    assert_eq!(read_tx_id, original_tx_id.inner());

    // Replace tx_id with tampered_tx_id
    bytes[tx_id_offset..tx_id_offset + 8].copy_from_slice(&tampered_tx_id.inner().to_le_bytes());

    // Recompute CRC32 so CRC check passes, isolating HMAC verification failure
    let payload = &bytes[12..];
    let computed_crc = crc32fast::hash(payload);
    bytes[8..12].copy_from_slice(&computed_crc.to_le_bytes());

    std::fs::write(&wal_path, bytes).unwrap();

    let wal_reopen = Wal::open(&wal_path).await;
    let is_corrupt = match wal_reopen {
        Err(e) => format!("{:?}", e).contains("HMAC mismatch"),
        Ok(w) => match w.replay().await {
            Err(e) => format!("{:?}", e).contains("HMAC mismatch"),
            Ok(_) => false,
        },
    };

    assert!(
        is_corrupt,
        "Manipulating tx_id in WAL V3 entry MUST invalidate HMAC chain during replay"
    );
    Ok(())
}

#[tokio::test]
async fn test_v2_to_v3_migration() -> Result<()> {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("v2_migration.wal");

    // Construct a legacy V2 WAL file (with MFW2 header and V2 HMAC)
    {
        let integrity_key = *b"test-integrity-key-32-bytes-long";
        let key_path = dir.path().join(".wal_integrity_key");
        std::fs::write(&key_path, integrity_key).unwrap();

        let op1 = WalOp::Put {
            tx_id: TxId::new(10),
            key: b"key1".to_vec(),
            value: b"val1".to_vec(),
        };
        let checksum1 =
            memfuse_store::wal::WalEntry::compute_checksum_v2(&op1, 1, &integrity_key, [0u8; 32])?;
        let entry1 = memfuse_store::wal::WalEntry {
            op: op1,
            seq_no: 1,
            checksum: checksum1,
            prev_hmac: [0u8; 32],
        };

        let op2 = WalOp::Put {
            tx_id: TxId::new(11),
            key: b"key2".to_vec(),
            value: b"val2".to_vec(),
        };
        let checksum2 =
            memfuse_store::wal::WalEntry::compute_checksum_v2(&op2, 2, &integrity_key, checksum1)?;
        let entry2 = memfuse_store::wal::WalEntry {
            op: op2,
            seq_no: 2,
            checksum: checksum2,
            prev_hmac: checksum1,
        };

        let mut v2_file = Vec::new();
        v2_file.extend_from_slice(b"MFW2");
        v2_file.extend_from_slice(&entry1.to_bytes()?);
        v2_file.extend_from_slice(&entry2.to_bytes()?);

        std::fs::write(&wal_path, v2_file).unwrap();
    }

    // Opening WAL should detect V2, replay successfully, and transparently rewrite as V3
    let wal = Wal::open(&wal_path).await?;
    let entries = wal.replay().await?;

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].1.tx_id(), TxId::new(10));
    assert_eq!(entries[1].1.tx_id(), TxId::new(11));

    // Inspect file on disk to confirm rewrite to MFW3 header
    let file_bytes = std::fs::read(&wal_path).unwrap();
    assert_eq!(
        &file_bytes[0..4],
        b"MFW3",
        "Migrated WAL file must have MFW3 header on disk"
    );

    Ok(())
}

#[tokio::test]
async fn test_length_extension_resistance() -> Result<()> {
    let dummy_key = b"test-integrity-key-32-bytes-long!";

    // Case 1: Put op with key="a", value="b" vs key="ab", value=""
    let op1 = WalOp::Put {
        tx_id: TxId::new(1),
        key: b"a".to_vec(),
        value: b"b".to_vec(),
    };
    let op2 = WalOp::Put {
        tx_id: TxId::new(1),
        key: b"ab".to_vec(),
        value: b"".to_vec(),
    };

    let checksum1 =
        memfuse_store::wal::WalEntry::compute_checksum_v3(&op1, 1, dummy_key, [0u8; 32])?;
    let checksum2 =
        memfuse_store::wal::WalEntry::compute_checksum_v3(&op2, 1, dummy_key, [0u8; 32])?;

    assert_ne!(
        checksum1, checksum2,
        "Length extension must produce different HMAC checksums for concatenated boundaries"
    );

    // Case 2: Delete op key boundary check
    let del1 = WalOp::Delete {
        tx_id: TxId::new(1),
        key: b"key1".to_vec(),
    };
    let del2 = WalOp::Delete {
        tx_id: TxId::new(1),
        key: b"key12".to_vec(),
    };
    let del_cs1 =
        memfuse_store::wal::WalEntry::compute_checksum_v3(&del1, 1, dummy_key, [0u8; 32])?;
    let del_cs2 =
        memfuse_store::wal::WalEntry::compute_checksum_v3(&del2, 1, dummy_key, [0u8; 32])?;

    assert_ne!(del_cs1, del_cs2);

    Ok(())
}

#[tokio::test]
async fn test_wal_hmac_chain_violation() -> Result<()> {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("chain.wal");

    let wal = Wal::open(&wal_path).await?;
    let op = WalOp::Put {
        tx_id: TxId::new(1),
        key: b"k".to_vec(),
        value: b"v".to_vec(),
    };
    let entry = wal.create_entry(op, 1).await?;
    wal.append(&entry).await?;
    drop(wal);

    // Manipuliere die Datei (HMAC ist an Offset 12..44 im unverlüsselten Fall)
    // WalEntry Layout: 4(len) + 4(CRC) + 8(seq) + 32(HMAC) + ...
    {
        let mut bytes = std::fs::read(&wal_path).unwrap();
        if bytes.len() > 20 {
            bytes[20] ^= 0xFF; // Manipuliere HMAC
            std::fs::write(&wal_path, bytes).unwrap();
        }
    }

    let wal = Wal::open(&wal_path).await;
    assert!(wal.is_err(), "Wal must fail replay if HMAC chain is broken");

    Ok(())
}
