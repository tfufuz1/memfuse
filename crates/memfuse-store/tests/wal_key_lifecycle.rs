use memfuse_store::wal::Wal;
use tempfile::tempdir;

/// Fault-Injection: 0-Byte-Datei wird korrekt als Fehler erkannt
#[tokio::test]
async fn test_empty_key_file_returns_error() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join(".wal_integrity_key");
    tokio::fs::write(&key_path, b"").await.unwrap(); // Simulate crash: 0-byte file
    let result = Wal::open(&dir.path().join("wal.log")).await;
    assert!(
        result.is_err(),
        "0-byte key file must return Err, not silently create wrong key"
    );
}

/// Race-Condition: AlreadyExists wird korrekt gehandhabt
#[tokio::test]
async fn test_concurrent_key_creation_consistent() {
    use tokio::task::JoinSet;
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("wal.log");
    // Simuliere 5 gleichzeitige Öffnungen
    let mut set = JoinSet::new();
    for _ in 0..5 {
        let path = wal_path.clone();
        set.spawn(async move { Wal::open(&path).await });
    }
    let mut keys = Vec::new();
    while let Some(res) = set.join_next().await {
        let wal = res.unwrap().unwrap();
        keys.push(wal.integrity_key_for_test().unwrap()); // Expose key via test helper
    }
    // Alle 5 müssen denselben Schlüssel gelesen haben
    let first = &keys[0];
    for k in &keys[1..] {
        assert_eq!(
            k, first,
            "Concurrent key creation must yield identical keys"
        );
    }
}

/// Persistenz: Schlüssel übersteht Prozess-Neustart
#[tokio::test]
async fn test_key_survives_restart() {
    let dir = tempdir().unwrap();
    let wal_path = dir.path().join("wal.log");
    let wal1 = Wal::open(&wal_path).await.unwrap();
    let key1 = wal1.integrity_key_for_test().unwrap();
    drop(wal1);
    let wal2 = Wal::open(&wal_path).await.unwrap();
    let key2 = wal2.integrity_key_for_test().unwrap();
    assert_eq!(key1, key2, "Key MUST be identical across restarts");
}
