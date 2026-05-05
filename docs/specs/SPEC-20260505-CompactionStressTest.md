# SPEC-20260505-CompactionStressTest

## 🎯 1. Das Ziel (Context & "Why")
Stresstest für die Write-Amplification und Compaction der Storage Engine. Ziel ist es zu beweisen, dass unter starker Last (viele Inserts + Deletes) die Size-Tiered Compaction reibungslos im Hintergrund läuft, ohne Lesevorgänge zu blockieren, und dass Tombstones korrekt abgeräumt werden.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-C1]**: Tombstone-GC darf niemals Daten verwerfen, die von einem aktiven Snapshot (`min_active_seqno`) noch gelesen werden.
- **[INV-C2]**: Parallele Read-Queries dürfen während des atomic SSTable-Swaps nicht fehlschlagen oder stottern.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-store`
- **File**: `src/compaction.rs` (Tests im bestehenden Modul)

```rust
// Erwartete Signatur des Tests:
#[tokio::test(flavor = "multi_thread")]
async fn test_compaction_stress_and_gc()
```

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- Der Test wird paniccen, falls Datensätze nach dem Compaction-Swap in gleichzeitigen Suchanfragen nicht auffindbar sind.
- Fehlerhafte Lock-Hierarchien würden im Multi-Thread-Tokio-Scheduler zu Deadlocks führen.

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
Der Test fügt 10.000 generierte Dokumente ein, löscht 5.000 davon, triggert mehrfache Background-Compactions via Loop und liest parallel konstant Daten aus. Der Test schlägt fehl, wenn (a) gelöschte Daten im Leser auftauchen, (b) ungelöschte Daten verloren gehen, oder (c) die Anzahl der SSTables nicht signifikant abnimmt.
