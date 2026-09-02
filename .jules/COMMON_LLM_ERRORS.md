# MemFuse — Häufige LLM-Fehler & Korrekturen
> Destilliert aus Session-Audits. Bei Unsicherheit: diese Datei zuerst prüfen.

## FEHLER-KLASSE 1: API-Halluzination

**Symptom**: Agent generiert Methodenaufruf der nicht existiert oder
falsche Signatur hat.

**Häufige Halluzinationen in diesem Projekt**:
```rust
// ❌ HALLUZINATION — collection.search() existiert nicht so:
collection.search("query", 10).await?

// ✅ KORREKT — aus collection.rs Signatur lesen:
collection.hybrid_search(query_text, query_vector, limit, filters).await?

// ❌ HALLUZINATION — RRF mit execute_rrf():
execute_rrf(results, k)

// ✅ KORREKT — aus fusion.rs:
reciprocal_rank_fusion(result_sets, max_results)
// oder: weighted_reciprocal_rank_fusion(weighted_sets, max_results)

// ❌ HALLUZINATION — SessionPool öffentlich zugreifen:
let pool = SessionPool::new(config)?;

// ✅ KORREKT — SessionPool ist pub(crate) in memfuse-embed:
// Aus externem Crate nicht direkt nutzbar. CrossEncoderReranker
// hält seinen eigenen internen Pool.
```

**Heilmittel**: Vor JEDEM Methodenaufruf:
```bash
grep -n "pub fn <METHODE_NAME>" crates/<crate>/src/*.rs
```

## FEHLER-KLASSE 2: Typ-Duplikation

**Symptom**: Agent legt neuen Typ an, der bereits existiert.

**Bekannte Duplikations-Risiken**:
```
ContextChunk     → memfuse-core/src/types/saos.rs (NICHT neu anlegen!)
SearchResult     → memfuse-core/src/types/ (NICHT neu anlegen!)
TxId             → memfuse-core/src/types/ (Newtype u64, NICHT redefinen!)
DocId            → memfuse-core/src/types/ (Blake3-Hash, NICHT redefinen!)
MemFuseError     → memfuse-core/src/error.rs (EINZIGE Error-Enum)
CheckpointGuard  → memfuse-store/src/checkpoint.rs (NICHT mit PersistentCheckpointStore verwechseln!)
```

**Heilmittel**: VOR jedem `struct` oder `enum`:
```bash
grep -rn "struct <TYPNAME>\|enum <TYPNAME>\|type <TYPNAME>" crates/ --include="*.rs"
grep "<TYPNAME>" docs/TYPE_REGISTRY.md
```

## FEHLER-KLASSE 3: Stille Fehler (Silent Failures)

**Symptom**: Agent schreibt Code der Fehler verschluckt.

```rust
// ❌ FALSCH — IO-Fehler verschluckt:
let _ = file.sync_all().await;
let _ = dir.sync_all();

// ❌ FALSCH — Deserialisierung mit Default-Fallback:
let entry = bincode::deserialize(&bytes).unwrap_or_default();

// ❌ FALSCH — Unhandled Result:
collection.insert(key, doc).await; // kein ?

// ✅ KORREKT — Fehler propagieren:
file.sync_all().await.map_err(|e| MemFuseError::Storage(...))?;
let entry = bincode::deserialize(&bytes)
    .map_err(|e| MemFuseError::ParseError(format!("WAL corrupt: {e}")))?;
collection.insert(key, doc).await?;
```

## FEHLER-KLASSE 4: DAG-Verletzungen

**Symptom**: Agent importiert ein Crate aus einer höheren Schicht.

```toml
# ❌ FALSCH — memfuse-core importiert memfuse-db (Layer 2 in Layer 0):
# In crates/memfuse-core/Cargo.toml:
memfuse-db = { path = "../memfuse-db" }  # ARCH-BRUCH!

# ❌ FALSCH — memfuse-store importiert memfuse-index (Layer 1 → Layer 1 Peer):
memfuse-index = { path = "../memfuse-index" }  # LAYER-PEER-BRUCH!
```

**Heilmittel**: Vor jeder `Cargo.toml`-Änderung:
```bash
just dag-check
```

## FEHLER-KLASSE 5: TxId-Missbrauch

```rust
// ❌ FALSCH — SystemTime als TxId:
let tx_id = SystemTime::now()
    .duration_since(UNIX_EPOCH).unwrap()
    .as_nanos() as u64;

// ❌ FALSCH — Manuell zählen:
let tx_id = self.counter.fetch_add(1, Ordering::SeqCst);

// ✅ KORREKT — immer via Collection-Allocator:
let tx_id = collection.allocate_tx().await?;
```

## FEHLER-KLASSE 6: unsafe ohne SAFETY-Kommentar

```rust
// ❌ FALSCH — unsafe ohne Beweis:
unsafe { ptr::copy_nonoverlapping(src, dst, len) }

// ✅ KORREKT:
// SAFETY: `src` und `dst` überlappen nicht (getrennte Allokationen durch
//         den Rust-Allokator). Länge `len` wurde durch Caller `hnsw.rs:347`
//         gegen beide Slice-Längen geprüft (assert_eq! in debug builds).
unsafe { ptr::copy_nonoverlapping(src, dst, len) }
```

**unsafe ist NUR erlaubt in** (AGENTS.md §4):
- `crates/memfuse-index/src/distance.rs` (SIMD)
- `crates/memfuse-index/src/diskann.rs` (Mmap)
- `crates/memfuse-index/src/persistence.rs` (Mmap)

## FEHLER-KLASSE 7: Test-Mirroring

```rust
// ❌ TEST-MIRRORING — Test spiegelt die Implementierungsformel:
let expected = (a - b).powi(2).sqrt(); // Gleiche Formel wie compute()!
assert!((result - expected).abs() < 1e-6); // Tautologie!

// ✅ KORREKT — Referenzwert unabhängig berechnet/recherchiert:
// Euklidische Distanz von [1,0,0] und [0,1,0] = sqrt(2) ≈ 1.41421356
assert!((result - 1.41421356_f32).abs() < 1e-4);
```

## FEHLER-KLASSE 8: Tag-Format-Verstöße

```rust
// ❌ FALSCH — fehlendes TS: und SESSION:
// AI-TAG[SMELL][CRITICAL] Problem in dieser Funktion (ID: AGT-STORE-001)

// ✅ KORREKT — vollständige Pflichtfelder:
// AI-TAG[SMELL][CRITICAL] Problem in dieser Funktion (ID: AGT-STORE-a3f29c1d) (TS: 2026-08-29T10:00:00Z) (SESSION: a3f29c1d)
// BEFUND: <Detaillierte Analyse>
// RISIKO: <Was passiert bei Ausfall>
// EMPFEHLUNG: <Konkrete Handlung>
```

## FEHLER-KLASSE 9: Stale Audit Findings implementieren

**Symptom**: Agent implementiert Fixes für Probleme die bereits behoben wurden.

**Regel** (aus `.jules/AUDIT_INTAKE_PROTOCOL.md`):
1. Datei + Zeile öffnen
2. Problem im AKTUELLEN Code prüfen
3. Falls nicht mehr vorhanden: als `[ENTKRÄFTET]` markieren
4. NIEMALS blind aus veralteten Audit-Prompts implementieren

## FEHLER-KLASSE 10: Dokument-Status selbst bewerten

```markdown
<!-- ❌ FALSCH — Agent bewertet Status ohne CI-Beweis: -->
| `memfuse-mcp` | 🟢 Clean |

<!-- ✅ KORREKT — Status NUR aus CI: -->
<!-- Status-Indikatoren werden AUSSCHLIESSLICH durch cargo xtask sync-docs
     aus CI-Ergebnissen gesetzt. Niemals manuell auf 🟢 setzen. -->
```

## FEHLER-KLASSE 11: Lock-Guard über .await halten

**Symptom**: Agent hält einen `RwLockReadGuard` oder `MutexGuard` über einen asynchronen Aufruf hinweg.

```rust
// ❌ FALSCH — Guard blockiert den Tokio Executor:
let db = state.db.read();
db.search(...).await?;

// ✅ KORREKT — Guard droppen vor dem await:
let collection = { state.db.read().collection("test")?.clone() };
collection.search(...).await?;
```

## FEHLER-KLASSE 12: Feature-Gate-Vergessen (onnx)

**Symptom**: Agent verwendet Code aus `memfuse-embed` ohne Feature-Flag.

```rust
// ❌ FALSCH:
use memfuse_embed::TextEmbedder; // Bricht Builds ohne onnx-Feature!

// ✅ KORREKT:
#[cfg(feature = "onnx")]
use memfuse_embed::TextEmbedder;
```

## FEHLER-KLASSE 13: Modul-Pfad-Halluzination

**Symptom**: Agent rät Modulpfade, anstatt in der Dateistruktur nachzusehen.

```rust
// ❌ FALSCH — Halluzinierter Pfad:
use memfuse_db::collection::Collection;

// ✅ KORREKT — Tatsächlicher Pfad (vorher grep nutzen):
use memfuse_db::Collection;
```
