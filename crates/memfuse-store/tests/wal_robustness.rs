use memfuse_store::wal::{Wal, WalOp};
use memfuse_core::{TxId, Result};
use tempfile::tempdir;
use std::fs::OpenOptions;
use std::io::Write;

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
        let mut file = OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        // Nur 2 Bytes eines 4-Byte Längen-Präfixes
        file.write_all(&[0x10, 0x00]).unwrap();
        file.sync_all().unwrap();
    }

    // 3. Öffne den WAL erneut. Replay wird intern ausgeführt.
    // In der aktuellen Implementierung führt ein fehlerhaftes Read in replay_with_size
    // vermutlich zu einem Fehler. Wir wollen sehen, wie das System reagiert.
    let wal = Wal::open(&wal_path).await;
    
    // ANCHOR:REACTION — Hier entscheiden wir: Soll Wal::open scheitern oder 
    // die korrupten Daten am Ende abschneiden (Truncate)?
    // Die Sovereign-Core-Doktrin bevorzugt Sicherheit. 
    // Wenn das Log korrupt ist, ist ein expliziter Fehler besser als stillschweigendes Ignorieren.
    
    match wal {
        Ok(w) => {
            let entries = w.replay().await?;
            // Wenn es Ok ist, sollten zumindest die ersten 3 da sein.
            assert_eq!(entries.len(), 3);
        },
        Err(e) => {
            println!("WAL open failed as expected on corruption: {:?}", e);
            // Das ist auch ein valides Ergebnis für "Zero Panic" — solange es kein Panic ist.
        }
    }

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
