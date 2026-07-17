# GLOSSARY.md — Domänenvokabular (Verbindlich)

Dieses Glossar definiert die exakten Fachbegriffe der MemFuse-Domain. Abweichungen im Quellcode oder in Instruktionen sind verboten, um Begriffsdrift zwischen Agenten-Sessions zu verhindern.

## Kern-Entitäten

| Begriff | Definition | Nicht verwenden |
|---|---|---|
| **Collection** | Logisch isolierter Namensraum (Vektoren + Text + Metadaten). | database, table, index |
| **Namespace** | Synonym für Collection auf LSM-Persistenzebene. | shard, partition |
| **DocId** | Eindeutige ID aus Blake3-Hash des Keys. | document ID, row ID |

## Transaktionen & Sequenzierung

| Begriff | Definition | Nicht verwenden |
|---|---|---|
| **TxId** | Transaktions-ID (`u64` Newtype). | transaction number |
| **SeqNo** | Monotone Sequenznummer (Bit 63 = Tombstone-Flag). | LSN, version |
| **TxBuffer** | Transaktions-Staging-Bereich vor dem WAL/MemTable-Commit. | write buffer, batch |
| **Snapshot Isolation** | Garantie, dass eine Transaktion einen konsistenten Zustand sieht — unabhängig von parallel laufenden Schreibern. | MVCC (unspezifisch) |
| **2PC** | Two-Phase Commit zur atomaren Transaktionskoordination über LSM + HNSW + BM25. | double phase commit |

## Persistenzschicht (LSM-Tree)

| Begriff | Definition | Nicht verwenden |
|---|---|---|
| **WAL** | Write-Ahead-Log für Crash-Consistency (CRC32- und HMAC-geschützt). | commit log, transaction log |
| **MemTable** | In-Memory-Schreibpuffer (BTreeMap) vor dem SSTable-Flush. | write buffer, mem index |
| **SSTable** | Sorted String Table. Immutables Speicherfile auf Disk mit Bloom-Filter und CRC32-Integrität. | data file, block file |
| **Compaction** | Hintergrundprozess zum Zusammenführen und Bereinigen von SSTables (Tombstone-Entfernung). | merge, GC |
| **Bloom-Filter** | Probabilistischer Existenz-Check in SSTables zur Vermeidung unnötiger Disk-Reads. | hash filter |
| **Tombstone** | Gelöscht-Markierung (Bit 63 der `SeqNo` ist auf 1 gesetzt). | delete marker, dead flag |

## Such- und Indexierungsschicht

| Begriff | Definition | Nicht verwenden |
|---|---|---|
| **HNSW** | Hierarchical Navigable Small World — Graph-basierte Vektor-Indexierung mit SIMD-beschleunigter Distanzberechnung. | vector tree, knn graph |
| **BM25** | Best Matching 25 — probabilistisches Relevanzmodell für Volltext-Suche (via `memfuse-text`). | tf-idf (veraltet) |
| **RRF** | Reciprocal Rank Fusion — Kombination von HNSW- und BM25-Rängen ohne Score-Normalisierung. | score merger, rank fusion |

## Fehlerbehandlung & Kryptographie

| Begriff | Definition | Nicht verwenden |
|---|---|---|
| **MemFuseError** | Zentrale Error-Enum in `memfuse-core`. Einziger Fehlertyp im gesamten Projekt. | Custom Error, anyhow |
| **AES-GCM-SIV** | Authentifizierte Verschlüsselung in `memfuse-crypto`. Nonce-Misuse-resistent. | AES-CBC, plaintext |
| **HMAC-Chaining** | WAL-Integritätsschutz: Jeder Eintrag hasht den vorherigen HMAC mit ein. | checksum chain |
