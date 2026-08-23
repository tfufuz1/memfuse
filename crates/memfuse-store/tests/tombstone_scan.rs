// Regression tests: `scan_prefix` must never return tombstoned entries.
//
// Without these tests a future refactoring of `scan_prefix_at` could silently
// drop the tombstone-filter (line ~908 in lsm.rs) and break `Collection::repair()`,
// which interprets every returned value as a `StoredDocument`.
//
// Two paths are exercised:
//   1. Pure MemTable path (no flush).
//   2. SSTable path (flush before the delete arrives in the MemTable).

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};

// ---------------------------------------------------------------------------
// 1. MemTable path — delete stays in the active MemTable
// ---------------------------------------------------------------------------

/// `scan_prefix` must exclude an entry that was deleted in a later, committed
/// transaction, even when the delete has not yet been flushed to an SSTable.
#[tokio::test]
async fn test_scan_prefix_excludes_tombstones() {
    let dir = tempfile::tempdir().unwrap();
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.unwrap();

    // Insert two keys under the same prefix.
    let tx1 = TxId::new(1);
    storage.put(tx1, b"ns:alive", b"alive_value").await.unwrap();
    storage.put(tx1, b"ns:dead", b"dead_value").await.unwrap();
    storage.commit(tx1).await.unwrap();

    // Delete one of them in a separate transaction.
    let tx2 = TxId::new(2);
    storage.delete(tx2, b"ns:dead").await.unwrap();
    storage.commit(tx2).await.unwrap();

    let results = storage.scan_prefix(b"ns:").await.unwrap();
    let keys: Vec<&[u8]> = results.iter().map(|(k, _)| k.as_slice()).collect();

    assert!(
        keys.contains(&b"ns:alive".as_slice()),
        "ns:alive muss im Scan-Ergebnis enthalten sein"
    );
    assert!(
        !keys.contains(&b"ns:dead".as_slice()),
        "ns:dead (Tombstone) darf NICHT im Scan-Ergebnis erscheinen"
    );
    assert_eq!(
        results.len(),
        1,
        "Genau ein Eintrag muss zurückgegeben werden"
    );
    assert_eq!(
        results[0].1, b"alive_value",
        "Der Wert des verbleibenden Eintrags muss korrekt sein"
    );
}

// ---------------------------------------------------------------------------
// 2. SSTable path — initial writes flushed before the delete is committed
// ---------------------------------------------------------------------------

/// Same invariant as above, but the live entries are already persisted in an
/// SSTable while the tombstone is still in the MemTable.  This exercises the
/// merge logic in `scan_prefix_at` that must prefer the higher-sequence
/// tombstone over the SSTable value.
#[tokio::test]
async fn test_scan_prefix_excludes_tombstones_after_flush() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LsmStorage::new(LsmConfig {
        path: dir.path().to_path_buf(),
        memtable_size_limit: 1, // Sofort flushen nach jedem Commit
        ..Default::default()
    })
    .await
    .unwrap();

    // Insert both keys and flush them to an SSTable.
    let tx1 = TxId::new(1);
    storage.put(tx1, b"ns:alive", b"v").await.unwrap();
    storage.put(tx1, b"ns:dead", b"v").await.unwrap();
    storage.commit(tx1).await.unwrap();
    storage.flush().await.unwrap();

    // Delete one key — this tombstone lives in the active MemTable while the
    // original entry resides in the SSTable.
    let tx2 = TxId::new(2);
    storage.delete(tx2, b"ns:dead").await.unwrap();
    storage.commit(tx2).await.unwrap();

    let results = storage.scan_prefix(b"ns:").await.unwrap();

    assert_eq!(
        results.len(),
        1,
        "Genau ein Eintrag muss zurückgegeben werden (Tombstone muss den SSTable-Eintrag überdecken)"
    );
    assert_eq!(
        results[0].0, b"ns:alive",
        "Der verbleibende Key muss ns:alive sein"
    );
}

// ---------------------------------------------------------------------------
// 3. Edge case — prefix scan on an entirely-deleted namespace returns empty
// ---------------------------------------------------------------------------

/// When *all* keys under a prefix are deleted, `scan_prefix` must return an
/// empty vector, not a vec of tombstone entries.
#[tokio::test]
async fn test_scan_prefix_all_deleted_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.unwrap();

    let tx1 = TxId::new(1);
    storage.put(tx1, b"col:a", b"1").await.unwrap();
    storage.put(tx1, b"col:b", b"2").await.unwrap();
    storage.commit(tx1).await.unwrap();

    let tx2 = TxId::new(2);
    storage.delete(tx2, b"col:a").await.unwrap();
    storage.delete(tx2, b"col:b").await.unwrap();
    storage.commit(tx2).await.unwrap();

    let results = storage.scan_prefix(b"col:").await.unwrap();
    assert!(
        results.is_empty(),
        "scan_prefix muss leer sein, wenn alle Einträge unter dem Prefix gelöscht wurden"
    );
}
