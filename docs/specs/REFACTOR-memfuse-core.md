# REFACTOR-PLAN: memfuse-core
**Datei:** `docs/specs/REFACTOR-memfuse-core.md`
**Erstellt:** 2026-05-28
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** Keine (Basis-Crate)

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 95% sauber    | 100%          |
| Skeleton-Anteil    | 7 Stellen     | 0             |
| Test-Coverage      | ~85%          | >90%          |
| API-Vollständigkeit| 70% (Dyn-Prob)| 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFOT)

#### FIND-COR-004: Dyn-Incompatibility of Core Traits
**Typ:** Architektur / Korrektheit
**Datei:** `crates/memfuse-core/src/traits.rs`
**Zeilen:** 63, 157, 222, 247
**Code (Kontext):**
```rust
pub trait StorageEngine: Send + Sync {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    // ...
}
```
**Problem:** Die Verwendung von `async fn` in Traits (AFIT) ohne `#[async_trait]` macht die Traits in der aktuellen Rust-Version nicht `dyn`-kompatibel (E0038). Dies blockiert Crates wie `memfuse-checkpoint`, die `Arc<dyn StorageEngine>` benötigen.
**Auswirkung:** Kompilierfehler in Downstream-Crates. Unmöglichkeit, Implementierungen zur Laufzeit zu tauschen.

**Refaktorisierungsanweisung:**
```
1. Füge #[async_trait] zu allen Core-Traits hinzu (StorageEngine, VectorIndex, TextIndex, GraphIndex).
2. Entferne #![allow(async_fn_in_trait)] aus lib.rs und traits.rs.
3. Stelle sicher, dass async-trait als Dependency in Cargo.toml vorhanden ist (ist bereits da).
```

**Akzeptanzkriterien:**
- [ ] `cargo check -p memfuse-checkpoint` ist grün.
- [ ] `Arc<dyn StorageEngine>` lässt sich instanziieren.

---

#### FIND-COR-002: Atomic Underflow in ResourceTracker
**Typ:** Datenkorruptheit / Panic
**Datei:** `crates/memfuse-core/src/types/budget.rs`
**Zeilen:** 66–69
**Code (Kontext):**
```rust
pub fn release_memory(&self, bytes: u64) {
    self.memory_used
        .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
}
```
**Problem:** `fetch_sub` auf `AtomicU64` prüft nicht auf Underflow. Wenn mehr Speicher freigegeben wird als belegt (z.B. durch Bug in Engine), wrappt der Wert auf ~18 Exabyte (`u64::MAX`).
**Auswirkung:** Alle zukünftigen Allokationsversuche schlagen permanent fehl (MemoryBudgetExceeded).

**Refaktorisierungsanweisung:**
```
1. Ersetze fetch_sub durch eine CAS-Loop (compare_exchange).
2. In der Loop: Berechne saturating_sub.
3. Logge eine Warnung, falls result < 0 wäre (Indiz für Bug in Engine).
```

**Akzeptanzkriterien:**
- [ ] Neuer Test `test_resource_tracker_underflow_protection` beweist, dass Wert nicht unter 0 fällt.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-COR-001: Trait Integrity (Dangerous Defaults)
**Typ:** Skeleton / Silent Failure
**Datei:** `crates/memfuse-core/src/traits.rs`
**Zeilen:** 95, 104, 115, 141
**Problem:** Default-Implementierungen wie `rollback` { Ok(()) } oder `scan` { Ok(Vec::new()) } führen zu silent failures, wenn ein Implementierer vergisst, sie zu überschreiben.
**Auswirkung:** Transaktions-Rollbacks "gelingen" scheinbar, tun aber nichts.

**Refaktorisierungsanweisung:**
```
1. Entferne Default-Implementierungen für kritische Methoden (rollback, rollback_to_tx, scan, stats).
2. Behalte Default-Impls nur dort, wo sie semantisch immer korrekt sind (z.B. empty search_filtered).
```

**Akzeptanzkriterien:**
- [ ] Alle Implementierungen (LsmStorage, HnswIndex etc.) müssen diese Methoden explizit implementieren.

---

### MEDIUM (Post-Launch — Tech-Debt Sprint)

#### FIND-COR-003: Pure Core Violation (Tokio Repo)
**Typ:** Architektur
**Datei:** `crates/memfuse-core/src/tx_buffer.rs`
**Zeilen:** 212–237
**Problem:** `memfuse-core` hängt von `tokio` ab, um den `orphan_reaper` zu starten. `lib.rs` deklariert jedoch "No I/O, no async".
**Auswirkung:** Core ist schwergewichtiger als nötig. Zirkuläre Logik-Abhängigkeiten.

**Refaktorisierungsanweisung:**
```
1. Verschiebe start_orphan_reaper in memfuse-db oder ein spezielles memfuse-runtime Crate.
2. Entferne tokio aus den regulären Dependencies von memfuse-core (nur in dev-dependencies behalten).
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-COR-004 (Löst Blockade für andere Crates)
Schritt 2: FIND-COR-002 (Kritische Stabilität)
Schritt 3: FIND-COR-001 (Erzwingt Korrektheit in Engines)
Schritt 4: FIND-COR-003 (Architektur-Cleanup)
```

## NEUE TESTS

```rust
// TEST-1: test_resource_tracker_underflow
// Prüft: tracker.release_memory(100) bei Stand 50 resultiert in 0, nicht MAX.

// TEST-2: test_dyn_storage_engine
// Prüft: Box<dyn StorageEngine> kann Methoden aufrufen.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Alle BLOCKING-Findings behoben (besonders E0038).
- [ ] `cargo check -p memfuse-core` und Downstream.
- [ ] `just triple-test` 3× grün.
- [ ] Testabdeckung für ResourceTracker Underflow vorhanden.
