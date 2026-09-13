# KV-Cache-Bridge Subsystem Audit Report

---
## KV-Cache-Bridge-Sub-Audit 2026-09-13T01:25:50Z

### Invarianten-Status & Befunde

- **INV-1 Fail-Open bei Cache-Miss/Krypto-Fehler (kein Err an Aufrufer)**: [OK]
  - **Befund/Analyse**: In `KvBridgeAdapter::try_get_cached_segment`, `match self.store.get_decrypted_segment(...)` fängt `Err(err)` ab, protokolliert es via `tracing::debug!`, und gibt `None` zurück. Dadurch fällt `generate_with_context()` sauber auf den vollständigen Prefill zurück, anstatt Fehler mit `?` an den Aufrufer zu propagieren.

- **INV-2 ModelFingerprint-Mismatch als Cache-Miss behandelt (nicht als Fehler)**: [BEFUND: AGT-CANDLE-fb44dd85]
  - **Befund/Analyse**: `try_get_cached_segment` akzeptiert den `_fingerprint: &ModelFingerprint` Parameter, ignoriert diesen jedoch bei der Segment-Abfrage in `TenantIsolatedKvStore::get_decrypted_segment`. Zwar schützt die AEAD-Entschlüsselung in `KvSegmentCipher::decrypt` vor der fälschlichen Nutzung von Daten anderer Fingerprints (führt zu CryptoError und dadurch zu `None`), jedoch sollte ein Fingerprint-Mismatch explizit vorab als Cache-Miss behandelt werden. `AI-TAG[SECURITY][MAJOR]` in `kv_bridge.rs:58` erfasst.

- **INV-3 Tenant-Isolation im KV-Segment-Store lückenlos**: [OK]
  - **Befund/Analyse**: `TenantIsolatedKvStore` speichert Segmente in einer `RwLock<AHashMap<TenantId, Vec<KvSegment>>>`. `get_decrypted_segment` und `get_segments` greifen ausschließlich über die übergebene `TenantId` auf das jeweilige Tenant-Segment-Vector zu. Es existieren keine globalen oder mandantenübergreifenden Lookup-Pfade.

- **INV-4 Kein Lock über .await, Semaphore-Backpressure respektiert**: [OK]
  - **Befund/Analyse**: `KvBridgeAdapter` nutzt synchrone Atomic-Operationen (`AtomicU64`) und synchrone `parking_lot::RwLock`-Guards in `TenantIsolatedKvStore`, die nicht über `async await`-Punkte hinweg gehalten werden. Der Semaphore-Backpressure-Vertrag aus `inference.rs` (`max_concurrent_inferences`) bleibt unberührt.

- **INV-5 EvictionWorker auf dediziertem OS-Thread, kein Async-Blocking**: [OK]
  - **Befund/Analyse**: `EvictionWorker::spawn` startet einen dedizierten OS-Thread via `std::thread::Builder::new().spawn(...)` mit einem 100ms Eviction-Loop. Der Tokio Async Executor wird zu keinem Zeitpunkt durch Eviction- oder Zeroizing-Schleifen blockiert.

- **INV-6 EncryptedKvLayer-Versionsfehler sprechend statt generisch**: [OK]
  - **Befund/Analyse**: `KvSegmentCipher::decrypt` vergleicht `encrypted.format_version != CURRENT_KV_FORMAT_VERSION` (Version 2) und gibt bei Abweichung den spezifischen Fehler `CryptoError::KvFormatVersionMismatch { expected, found }` zurück.

---
*Audit durchgeführt von Session 38de9c27 am 2026-09-13.*
