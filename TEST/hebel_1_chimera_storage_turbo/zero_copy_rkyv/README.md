# Zero-Copy Serialization (rkyv) — Integration Guide für MemFuse

## 1. Technischer Hintergrund & Synergie
MemFuse verwendet aktuell `serde` und `bincode` für die Serialisierung im Storage-Layer und FFI-Grenzen. Während `bincode` schnell ist, erfordert es bei jedem Lesevorgang die Allokation von Heap-Speicher (`Vec<u8>`, `String`, Deserialisierung in neue Rust-Structs).

**Project Chimera** hat dieses Problem mit `rkyv` gelöst:
- **Zero-Copy Parsing:** Mapped Speicherbereiche (z. B. aus Memory-Mapped SSTables oder Shared Memory) werden ohne Allokation oder Deserialisierung direkt als referenzierte Typen (`&ArchivedType`) gelesen.
- **Bytecheck-Validierung:** Mit `check_archived_root` (bzw. dem Wrapper `AliasedBytes`) wird die Integrität und Alignment formal validiert, bevor auf den Speicher zugegriffen wird.
- **Latenz-Vorteil:** Bis zu **10x bis 50x schnellere Zugriffe** im Hot-Path (Vektor-Metadaten-Filterung, Point-Lookups und LSM MemTable-Scans).

## 2. Extrahierte Chimera-Komponenten
Die folgenden produktionserprobten Dateien wurden aus `chimeraDB` in diesen Ordner extrahiert:

| Datei | Quelle | Relevanz für MemFuse |
|:---|:---|:---|
| [`aliased_bytes.rs`](./aliased_bytes.rs) | `chimera-core/src/util.rs` | Sicherer `AliasedBytes<'a>` Zero-Copy-Casting-Wrapper mit `bytecheck` |
| [`rkyv_types.rs`](./rkyv_types.rs) | `chimera-core/src/types.rs` | Zero-Copy Namespace-, Identifier- und Dokument-Typen |
| [`rkyv_tx_buffer.rs`](./rkyv_tx_buffer.rs) | `chimera-core/src/tx_buffer.rs` | Atomic Transaktionspuffer mit `rkyv::Archive` Derivaten |
| [`rkyv_hnsw_persist.rs`](./rkyv_hnsw_persist.rs) | `chimera-index-vector/src/persist.rs` | Persistente HNSW-Strukturen für direktes Mmap-Lesen ohne Heap-Allokation |
| [`rkyv_metadata_index.rs`](./rkyv_metadata_index.rs) | `chimera-index-metadata/src/lib.rs` | Metadaten-Indexierungsstrukturen mit Zero-Copy Filterung |
| [`rkyv_lsm_storage.rs`](./rkyv_lsm_storage.rs) | `chimera-storage/src/lsm.rs` | Zero-Copy LSM Payload-Handling und MemTable Batches |

## 3. Implementierungsplan für MemFuse
1. **Dependency hinzufügen (`Cargo.toml`):**
   ```toml
   rkyv = { version = "0.7", features = ["validation", "alloc"] }
   bytecheck = "0.6"
   ```
2. **Hot-Path 1: Metadaten-Filterung (`memfuse-db`):**
   - Beim Scannen von Metadaten-Feldern in Filtern (`filter_by_metadata`) nicht mehr ganze JSON-Objekte deserialisieren, sondern archivierte Byte-Slices via `AliasedBytes::cast::<ArchivedMetadata>()` scannen.
3. **Hot-Path 2: WAL V3 & MemTable:**
   - WAL-Einträge im Zero-Copy-Format schreiben. Beim Startup oder Recovery Replay können WAL-Segmente memory-mapped und direkt gelesen werden, ohne Millionen Structs auf dem Heap zu erzeugen.
