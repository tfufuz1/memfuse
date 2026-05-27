# Jules Agent Remediation Prompts

Stand: 2026-05-27
Zweck: Systematische Auflösung der im Forensic Audit (v2.0-Alpha) identifizierten Skeletons durch das Autonomous Squad (Jules).

---

## 1. Prompt für Agent 10 (Security Engineer)
**Crate**: `memfuse-crypto`
**Vulnerability**: CRIT-002 — WAL Encryption Bypass

```markdown
# MISSION
Du bist Agent 10 (Security Engineer). 
Im Forensic Audit (CRIT-002) wurde festgestellt, dass die Verschlüsselung für den WAL (Write-Ahead Log) in `crates/memfuse-crypto/src/wal_crypto.rs` umgangen wird. Aktuell ist `EncryptedWal::encrypt_chunk()` lediglich ein Stub, der Plaintext zurückgibt.

# DEIN ZIEL
Implementiere die fehlende Geschäftslogik für die AES-256-GCM Verschlüsselung im MemFuse WAL.

# AKTIONEN
1. Ersetze den Stub in `EncryptedWal::encrypt_chunk()` (und den zugehörigen Decrypt-Stub, falls vorhanden) so, dass die Daten tatsächlich sicher mit dem `KeyManager` / AES-256-GCM verschlüsselt werden.
2. Der Chunk muss mit einem Nonce inkl. der aktuellen `seq_no` / `offset` kryptografisch gebunden werden (Replay-Protection).
3. Beachte strikt die Sovereign Core Doctrine: Kein `unwrap()` oder `expect()`, `unsafe` ist verboten. Gib `MemFuseError` bei Fehlschlag zurück.
4. Schreibe einen Contract-Test (`#[tokio::test]`), der beweist, dass ein geschriebener Chunk verschlüsselt ist (darf nicht dem Plaintext entsprechen) und fehlerfrei wieder entschlüsselt werden kann.

# VERIFIKATION
Stelle sicher, dass `just triple-test` nach deiner Implementierung fehlerfrei durchläuft.
```

---

## 2. Prompt für Agent 02 (Store Engineer)
**Crate**: `memfuse-store`
**Vulnerability**: HIGH-003 — Missing WAL Truncation 

```markdown
# MISSION
Du bist Agent 02 (Store Engineer).
Im Forensic Audit (HIGH-003) wurde festgestellt, dass die Methode `rollback_to_tx` in `crates/memfuse-store/src/lsm.rs` bei einem fehlschlagenden Transaktions-Commit den State nur In-Memory zurückrollt, jedoch die `.log` Datei (den WAL) auf der Festplatte nicht physisch kürzt (truncate).

# DEIN ZIEL
Vollende die Rollback-Logik, sodass verwaiste (aborted) Einträge dauerhaft aus dem WAL gelöscht werden, um Ghost-Replays bei System-Neustart zu verhindern (Split-Brain Prevention).

# AKTIONEN
1. Erweitere das Interface von `Wal` in `crates/memfuse-store/src/wal.rs` um eine Methode `pub async fn truncate(&self, valid_size: u64) -> Result<()>`.
2. Diese Methode muss die physische Datei mithilfe von `tokio::fs::File::set_len` auf den sicheren `valid_size` (bzw. Offset) kürzen und das `size` Atomic aktualisieren.
3. Passe `LsmStorage::rollback_to_tx` in `lsm.rs` so an, dass diese neue `truncate` Methode des WALs aufgerufen wird. Der sichere Offset entspricht der WAL-Größe **vor** dem Beginn des fehlgeschlagenen Commits. (Ggf. müssen Offsets je TxId im MemTable/TxBuffer getrackt werden).
4. Einhaltung der Sovereign Core Doctrine: Kein Blockierendes I/O (`std::fs`), nutze `tokio`.

# VERIFIKATION
Füge einen Testfall in `wal.rs` oder `lsm.rs` hinzu, der einen partiellen Append durchführt, diesen truncatet, abspeichert und sicherstellt, dass beim anschließenden `replay()` die verworfenen Bytes nicht mehr vorhanden sind.
```

---

## 3. Prompt für Agent 01 (Core Guardian) & Squad
**Crate**: `memfuse-core` (sowie alle L1 Crates)
**Optimization**: OPT-001 — Async Trait Boxing Overhead

```markdown
# MISSION
Du bist Agent 01 (Core Guardian).
Im Forensic Audit (OPT-001) wurde identifiziert, dass wir in `crates/memfuse-core/src/traits.rs` das Makro `#[async_trait]` nutzen. Dies erzeugt Allokations-Overhead durch dynamisches Boxing auf dem Heap (`Pin<Box<dyn Future>>`), was in heißen Pfaden die Caching-Performance degradiert.

# DEIN ZIEL
Modernisiere das MemFuse Trait-System auf native async Traits (verfügbar seit Rust 1.75), um Zero-Cost-Abstractions zu garantieren.

# AKTIONEN
1. Entferne das Makro `#[async_trait]` über den Kern-Traits: `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex` in `crates/memfuse-core/src/traits.rs`.
2. Ersetze die Signatur durch native `async fn` Deklarationen.
3. Passe die Implementierungen quer über alle Layer-1 Crates an (insbesondere `LsmStorage` in `memfuse-store`, `HnswIndex` in `memfuse-index`, Inverted Index in `memfuse-text` und CSR Graph in `memfuse-graph`), indem du die dortigen `#[async_trait]` Annotationen entfernst.

# VERIFIKATION
Stelle sicher, dass `cargo clippy -- -D warnings` fehlerfrei bleibt und überprüfe per `just triple-test`, dass alle Compiler-Garantien bezüglich Send/Sync für die nativen async Futures in Tokio-Kontexten erfüllt sind. Ggf. müssen Lifetime- oder Send-Bounds explizit an die Trait-Methoden angehängt werden.
```
