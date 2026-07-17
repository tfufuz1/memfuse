# WAL & Crypto Invarianten

> Referenziert aus `AGENTS.md §8`

## WAL-First Regel

Kein Speicherzustand wird modifiziert, bevor der WAL-Eintrag physisch committed + synced ist.  
Reihenfolge: `WAL::append()` → `fsync()` → `MemTable::apply()`.

## HMAC-Chaining

```
Entry_N.checksum = HMAC(key, prev_hmac_N-1 || seq_N || op_type || payload)
```

`prev_hmac` der ersten Entry = `[0u8; 32]`.  
Bei Replay: Chain von Entry 0 bis letzte Entry validieren. Abbruch bei erstem Mismatch → `MemFuseError::WalCorruption`.

## Crypto-Isolation

- `memfuse-crypto` ist die **einzige** Stelle für Krypto-Primitiven (AES-GCM-SIV).
- Jede WAL-Datei hat ein eigenes Key (HKDF-Ableitung aus UUID-Sidecar).
- UUID-Sidecar: `<wal_path>.uuid` (16 Byte, raw). Muss vor erster WAL-Nutzung existieren.

## Was niemals passieren darf

- `bincode::deserialize(...).unwrap_or_default()` auf WAL-Einträge — Korruption wird zu Datenverlust.
- Krypto-Code außerhalb von `memfuse-crypto` (auch nicht „nur für diesen einen Fall").
