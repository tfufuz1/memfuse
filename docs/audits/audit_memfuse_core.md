# 🔬 Audit Report: `memfuse-core` — Sprint 2/3 Deep Analysis

**Scope**: All 15 files, ~1,895 LoC  
**Auditor**: Chef-Auditor (Sovereign Core Constitution)  
**Date**: 2026-06-24  
**Verdict**: ⚠️ **Architektonisch solide, aber mit 7 kritischen Contract-Lücken**

---

## 1. Das logische Sündenregister (Böse Überraschungen)

### FIND-CORE-S2-001 — 🔴 [scan_prefix_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#124-132) Default bricht Snapshot-Isolation

**Datei**: [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#L128-L131)  
**Schwachstelle**: ACID-Bruch & Isolation (Vektor 1)

```rust
async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    // Default: falls back to current scan_prefix (no isolation)
    self.scan_prefix(prefix).await
}
```

**Warum das schiefgeht**: Jeder [StorageEngine](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#71-140)-Implementor, der [scan_prefix_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#124-132) nicht aktiv überschreibt, liefert **Dirty Reads**. Der Caller (z.B. `memfuse-db` im Snapshot-Modus) glaubt, MVCC-isoliert zu lesen, bekommt aber den aktuellen, nicht-isolierten Zustand. Das ist kein "TODO" — es ist ein **stiller ACID-Bruch**, der unter Last dazu führt, dass Reads laufende Writes sehen.

**Realwelt-Szenario**: Thread A schreibt 50 Keys in TX-1. Thread B liest gleichzeitig mit Snapshot seq_no=100 via [scan_prefix_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#124-132). Der Default liefert Partial-Writes von TX-1 zurück → inkonsistentes Ergebnis, das in die 4-Signal-Fusion einfließt.

> [!CAUTION]
> **Schweregrad: KRITISCH** — Dieser Default ist eine architektonische Zeitbombe. Jeder neue StorageEngine-Implementor erbt stillschweigend das Dirty-Read-Verhalten.

---

### FIND-CORE-S2-002 — 🔴 `TextIndex::search_at` Default bricht Snapshot-Isolation

**Datei**: [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#L250-L253)  
**Schwachstelle**: ACID-Bruch & Isolation (Vektor 1)

```rust
async fn search_at(&self, query: &str, k: usize, _seq_no: u64) -> Result<Vec<ScoredDocument>> {
    // Default: falls back to current search (no isolation)
    self.search(query, k).await
}
```

**Identisches Problem**: BM25-Suche ignoriert die Snapshot-Semantik. Die 4-Signal-Fusion kombiniert dann einen vektoriellen Snapshot-Read mit einem nicht-isolierten Text-Read → **inkonsistente Fusion-Scores**.

---

### FIND-CORE-S2-003 — 🟡 `TxBuffer::reap_orphans` — Cross-Shard Sequential Lock-Acquisition

**Datei**: [tx_buffer.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#L181-L195)  
**Schwachstelle**: Concurrency & Deadlocks (Vektor 5)

```rust
pub fn reap_orphans(&self) -> Vec<TxId> {
    let mut expired = Vec::with_capacity(self.len()); // ← self.len() acquires 64 read locks
    for shard_lock in &self.shards {
        let mut shard = shard_lock.write();          // ← then acquires 64 write locks sequentially
        shard.ops.retain(|tx, (_, created)| { ... });
    }
    expired
}
```

**Warum das schiefgeht**: 
1. `self.len()` iteriert über **alle 64 Shards** mit Read-Locks.
2. Dann iteriert [reap_orphans](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#180-196) erneut über alle 64 Shards mit **Write-Locks**.
3. Zwischen [len()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#170-174) und der Write-Lock-Schleife gibt es eine **TOCTOU-Lücke**: Neue TXs können zwischen Read und Write hinzukommen.
4. Unter hoher Last hält der Reaper sequentiell 64 Write-Locks → **Lock-Contention-Spike**, der alle [stage()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#119-132)/[drain()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#149-162)-Aufrufe blockiert.

**Realwelt-Szenario**: Der Orphan-Reaper läuft alle 30s. Auf einem 16-Core-System mit 100 aktiven TXs blockiert er für die gesamte Sweep-Dauer alle 64 Shards sequentiell. Worst-Case: >1s Stall.

---

### FIND-CORE-S2-004 — 🟡 `TxBuffer::len()` / [is_empty()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#213-217) — Non-Atomic Multi-Shard Reads

**Datei**: [tx_buffer.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#L171-L178)  
**Schwachstelle**: Concurrency (Vektor 5)

```rust
pub fn len(&self) -> usize {
    self.shards.iter().map(|s| s.read().ops.len()).sum()
}
pub fn is_empty(&self) -> bool {
    self.shards.iter().all(|s| s.read().ops.is_empty())
}
```

**Warum das schiefgeht**: Jeder Shard wird **einzeln** gelocked. Zwischen Shard₀.read() und Shard₆₃.read() können TXs hinzugefügt oder entfernt werden. Das Ergebnis ist daher **nie ein konsistenter Snapshot** der Buffer-Größe. 

**Risiko**: Niedrig für einfache Metriken, aber **hoch wenn [len()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#170-174) als Steuerungsgröße für Backpressure verwendet wird** (z.B. "wenn len() > 1000, blockiere neue TXs"). In dem Fall: Race Condition → Buffer overload.

---

### FIND-CORE-S2-005 — 🟡 [VectorIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#152-221) / [GraphIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#283-315) / [TextIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#245-273) — Kein Snapshot-Aware Search

**Datei**: [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#L169-L170)  
**Schwachstelle**: ACID-Bruch (Vektor 1) + Cluster-Blindheit (Vektor 4)

```rust
async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;
```

**Warum das schiefgeht**: `VectorIndex::search` hat **keinen [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111)-Parameter**. Im Gegensatz zu `StorageEngine::get_at_seq` und `TextIndex::search_at` gibt es für den Vektor-Pfad **keine Möglichkeit, MVCC-isoliert zu suchen**. Die 4-Signal-Fusion mischt dadurch:
- Storage: ✅ isoliert (via [get_at_seq](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#380-383))
- Text: ⚠️ Default nicht isoliert (FIND-002)
- Vector: ❌ **strukturell unmöglich** zu isolieren
- Graph: ❌ **kein [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111) im Trait**

**Das bedeutet**: Snapshot-Isolation ist im Core-Contract **architektonisch unvollständig** für 3 von 4 Signalen.

---

### FIND-CORE-S2-006 — 🟡 [GraphIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#283-315) — Kein [last_tx_id()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#466-469) / [len()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#170-174) im Trait

**Datei**: [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#L283-L314)  
**Schwachstelle**: Cluster-Blindheit (Vektor 4) + Datenverlust (Vektor 2)

Der [GraphIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#283-315)-Trait hat **weder [last_tx_id()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#466-469) noch [len()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#170-174)**. Im Cluster-Kontext kann der Follower daher nicht feststellen:
- Welche TX er zuletzt repliziert hat (→ kann nicht idempotent replizieren)
- Ob der Graph-Index synchron zum LSM-Storage ist (→ Cluster-Blindheit)

[VectorIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#152-221) hat [last_tx_id()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#466-469), [TextIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#245-273) hat es nicht. **Inkonsistenz im Trait-Design**.

---

### FIND-CORE-S2-007 — 🟢 `SnapshotRegistry::release()` — Stiller No-Op bei unbekanntem [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111)

**Datei**: [snapshot.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#L80-L90)  
**Schwachstelle**: Tombstone-GC (Vektor 3)

```rust
pub(crate) fn release(&self, seq_no: u64) {
    let seq_no = seq_no & !TOMBSTONE_BIT;
    let mut active = self.active.lock();
    if let Some(count) = active.get_mut(&seq_no) {
        *count -= 1;
        if *count == 0 { active.remove(&seq_no); }
    }
    self.update_min(&active);
}
```

**Risiko**: Wenn [release()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#80-91) mit einem nie registrierten [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111) aufgerufen wird, passiert **nichts** — kein Fehler, kein Log. Ein Double-Free (SnapshotGuard + manueller [unpin](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#75-79)) für dieselbe [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111) reduziert den Ref-Count auf 0, entfernt den Eintrag, und ermöglicht **vorzeitige Tombstone-GC**. Der zweite [release](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#80-91)-Aufruf ist dann ein stiller No-Op.

**Realwelt-Szenario**: Ein Checkpoint-Pin wird per [unpin()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#75-79) entfernt, aber der zugehörige [SnapshotGuard](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#102-106) lebt noch. Bei Drop des Guards wird [release()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#80-91) erneut aufgerufen → No-Op (weil Eintrag schon weg). Der GC hat in der Zwischenzeit Daten gelöscht, die der Guard eigentlich schützen sollte.

> [!NOTE]
> Der aktuelle Code ist korrekt unter der Annahme, dass [pin()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#67-74) und [register()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#48-60) **nie für dieselbe [seq_no](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#108-111)** gleichzeitig verwendet werden. Diese Annahme ist aber **nirgends enforced**.

---

## 2. Der gehärtete Code (Review & Refactoring)

> [!IMPORTANT]
> Die folgenden Inline-Kommentare sind der Audit-Output. Sie markieren exakt die Stellen, die gehärtet oder überwacht werden müssen.

### [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs) — Trait-Contracts

```rust
// ═══════════════════════════════════════════════════════════════════
// StorageEngine Trait
// ═══════════════════════════════════════════════════════════════════

#[async_trait]
pub trait StorageEngine: Send + Sync + 'static {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

    // 🛡️ SICHERUNG: get_at_seq ist der EINZIGE korrekte Lesepfad für MVCC-Snapshots.
    // Jeder Caller, der Snapshot-Isolation braucht, MUSS diesen Pfad verwenden.
    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>>;

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()>;
    async fn commit(&self, tx_id: TxId) -> Result<()>;
    async fn rollback(&self, tx_id: TxId) -> Result<()>;
    async fn flush(&self) -> Result<()>;

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-001] Dieser Default liefert DIRTY READS.
    // Jeder Implementor, der scan_prefix_at NICHT überschreibt, bricht
    // stillschweigend die Snapshot-Isolation. Der Kommentar "no isolation"
    // im Code ist KEIN Freifahrtschein — er ist ein offener ACID-Bruch.
    //
    // REMEDIATION: Entweder (a) Default entfernen und Implementierung erzwingen,
    // oder (b) Default auf Err(Unimplemented) setzen statt stiller Fallback.
    async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64)
        -> Result<Vec<(Vec<u8>, Vec<u8>)>>
    {
        self.scan_prefix(prefix).await
    }
}

// ═══════════════════════════════════════════════════════════════════
// VectorIndex Trait
// ═══════════════════════════════════════════════════════════════════

#[async_trait]
pub trait VectorIndex: Send + Sync + 'static {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-005] search() hat KEINEN seq_no-Parameter.
    // Die 4-Signal-Fusion kann den Vektor-Pfad NICHT snapshot-isoliert abfragen.
    // Wenn Storage isoliert liest (get_at_seq) aber Vector ungefiltert sucht,
    // sieht die Fusion einen inkonsistenten Zustand.
    //
    // REMEDIATION: search_at(query, k, seq_no) mit Default-Impl analog zu
    // TextIndex::search_at hinzufügen. Langfristig: Pflicht-Override.
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;
    async fn last_tx_id(&self) -> Result<u64>;
    async fn len(&self) -> usize;
    async fn stats(&self) -> Result<VectorIndexStats>;
}

// ═══════════════════════════════════════════════════════════════════
// TextIndex Trait
// ═══════════════════════════════════════════════════════════════════

#[async_trait]
pub trait TextIndex: Send + Sync + 'static {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-002] Identisch zu FIND-001.
    // Default fällt auf nicht-isolierten search() zurück.
    // Jede Implementierung die search_at() nicht überschreibt,
    // liefert live-Daten statt Snapshot-Daten.
    async fn search_at(&self, query: &str, k: usize, _seq_no: u64)
        -> Result<Vec<ScoredDocument>>
    {
        self.search(query, k).await
    }

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-006] TextIndex hat kein last_tx_id().
    // Im Cluster-Kontext kann ein Follower nicht prüfen, ob sein
    // Text-Index synchron zum Raft-Log ist.
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;
    async fn stats(&self) -> Result<TextIndexStats>;
}

// ═══════════════════════════════════════════════════════════════════
// GraphIndex Trait
// ═══════════════════════════════════════════════════════════════════

#[async_trait]
pub trait GraphIndex: Send + Sync + 'static {
    // 🚨 VIBE-WARNING: [FIND-CORE-S2-005] traverse() hat KEINEN seq_no-Parameter.
    // Graph-Signal in der Fusion ist NICHT snapshot-isoliert.
    async fn traverse(
        &self,
        start_node: EntityId,
        max_hops: usize,
    ) -> Result<Vec<(EntityId, f32)>>;

    async fn add_entity(&self, tx: TxId, entity: Entity) -> Result<()>;
    async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()>;
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-006] Kein last_tx_id(), kein len().
    // Cluster-Replikation kann nicht prüfen ob Graph synchron ist.
    async fn stats(&self) -> Result<GraphIndexStats>;
}
```

### [snapshot.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs) — SnapshotRegistry

```rust
impl SnapshotRegistry {
    // 🛡️ SICHERUNG: register() maskiert TOMBSTONE_BIT, sodass ein seq_no mit
    // gesetztem Bit 63 korrekt als Basis-seq_no behandelt wird. Ohne diese
    // Maskierung würde ein Tombstone-seq_no als extrem hohe Nummer registriert
    // und NIE als min_active_seqno gelten → GC-Bypass.
    pub fn register(self: &Arc<Self>, seq_no: u64) -> SnapshotGuard {
        let seq_no = seq_no & !TOMBSTONE_BIT;
        let mut active = self.active.lock();
        *active.entry(seq_no).or_default() += 1;
        // 🛡️ SICHERUNG: update_min() wird INNERHALB des Lock-Scopes aufgerufen.
        // Das garantiert, dass min_active_seqno atomar zum BTreeMap-Zustand ist.
        self.update_min(&active);
        SnapshotGuard { registry: self.clone(), seq_no }
    }

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-007] release() ist ein stiller No-Op wenn
    // seq_no nicht in der Map existiert. Ein Double-Release (z.B. pin(50) + register(50)
    // → unpin(50) → Drop Guard) würde den Ref-Count zu früh auf 0 bringen.
    // Die GC könnte dann Tombstones für seq≥50 entfernen, obwohl der Guard
    // noch aktive Reads schützen soll.
    //
    // REMEDIATION: Debug-Assert oder Logging wenn seq_no nicht gefunden wird.
    pub(crate) fn release(&self, seq_no: u64) {
        let seq_no = seq_no & !TOMBSTONE_BIT;
        let mut active = self.active.lock();
        if let Some(count) = active.get_mut(&seq_no) {
            *count -= 1;
            if *count == 0 { active.remove(&seq_no); }
        }
        self.update_min(&active);
    }

    // 🛡️ SICHERUNG: update_min() nutzt BTreeMap::keys().next() → O(1) Zugriff
    // auf das Minimum. u64::MAX als Default ist KORREKT: Es erlaubt der Compaction,
    // ALLE Tombstones zu entfernen, da kein Reader aktiv ist.
    fn update_min(&self, active: &BTreeMap<u64, usize>) {
        let min = active.keys().next().copied().unwrap_or(u64::MAX);
        self.min_active_seqno.store(min, Ordering::Release);
    }
}

// 🛡️ SICHERUNG: Drop-Impl garantiert RAII-Deregistrierung.
// Ohne dieses Pattern müsste jeder Caller manuell release() aufrufen →
// ein vergessener Aufruf = permanentes GC-Block (min_active_seqno bleibt niedrig).
impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        self.registry.release(self.seq_no);
    }
}
```

### [tx_buffer.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs) — TxBuffer

```rust
impl<T: Clone> TxBuffer<T> {
    // 🛡️ SICHERUNG: shard_count=0 wird auf 1 korrigiert (§2 Zero-Panic).
    // Ohne diesen Guard → Division-by-Zero in shard_idx() → panic im Release-Build
    // (Integer-Division, kein IEEE-754 NaN wie bei f64).
    pub fn new_with_config(shard_count: usize, tx_timeout: Duration) -> Self {
        let shard_count = if shard_count == 0 { 1 } else { shard_count };
        // ...
    }

    // 🛡️ SICHERUNG: Modulo-Operation garantiert, dass shard_idx IMMER
    // im gültigen Bereich [0, shards.len()) liegt → kein Bounds-Check-Panic.
    #[inline]
    fn shard_idx(&self, tx: TxId) -> usize {
        (tx.inner() % self.shards.len() as u64) as usize
    }

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-003] reap_orphans() acquiriert SEQUENTIELL
    // alle 64 Write-Locks. Unter hoher Concurrency blockiert das ALLE stage()/drain()
    // Aufrufe für die Dauer des Sweeps. Zusätzlich: self.len() in der capacity-Berechnung
    // acquiriert nochmal 64 Read-Locks VORHER → 128 Lock-Acquisitions total.
    //
    // REMEDIATION: (a) Kapazität per Shard schätzen statt self.len(),
    // (b) Reaping shard-weise mit Pausen zwischen Shards,
    // (c) Try-Lock statt blockierendem Write-Lock.
    pub fn reap_orphans(&self) -> Vec<TxId> {
        let mut expired = Vec::with_capacity(self.len());
        for shard_lock in &self.shards {
            let mut shard = shard_lock.write();
            shard.ops.retain(|tx, (_, created)| {
                if created.elapsed() > self.tx_timeout {
                    expired.push(*tx);
                    false
                } else {
                    true
                }
            });
        }
        expired
    }

    // 🚨 VIBE-WARNING: [FIND-CORE-S2-004] len() iteriert über alle Shards mit
    // einzelnen Read-Locks. Das Ergebnis ist KEIN atomarer Snapshot:
    // Shard 0 wird gelesen (count=5), dann stage() auf Shard 0 → count wird 6,
    // aber len() gibt 5+... zurück. Für reine Metriken OK, aber NICHT als
    // Steuerungsgröße für Backpressure verwenden!
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().ops.len()).sum()
    }
}
```

---

## Zusammenfassung — Priorisierte Remediation

| Prio | Finding | Schweregrad | Aufwand | Nächster Schritt |
|------|---------|-------------|---------|------------------|
| 1 | FIND-001: [scan_prefix_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#124-132) Default | 🔴 KRITISCH | Klein | Default → `Err(Unimplemented)` oder Interface ohne Default |
| 2 | FIND-002: [search_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#249-254) Default | 🔴 KRITISCH | Klein | Identische Remediation wie FIND-001 |
| 3 | FIND-005: Kein [search_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#249-254) für Vector/Graph | 🟡 DESIGN | Mittel | [search_at(query, k, seq_no)](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#249-254) Methode in [VectorIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#152-221) + [GraphIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#283-315) |
| 4 | FIND-006: Missing [last_tx_id](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#466-469) in TextIndex/GraphIndex | 🟡 DESIGN | Klein | Trait-Methoden mit Default-Impl hinzufügen |
| 5 | FIND-003: [reap_orphans](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#180-196) Lock-Contention | 🟡 PERF | Mittel | Try-Lock + shard-weise Pausen |
| 6 | FIND-004: Non-atomic [len()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#170-174)/[is_empty()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#213-217) | 🟢 LOW | Klein | Dokumentation + keine Backpressure-Nutzung |
| 7 | FIND-007: Silent [release()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#80-91) No-Op | 🟢 LOW | Klein | `debug_assert!` oder Logging |

### KI-Test-Bluff Bewertung (Vektor 6)

| Bereich | Testabdeckung | Bewertung |
|---------|--------------|-----------|
| SnapshotRegistry | 5 Tests, inkl. Ref-Counting + Tombstone-Masking | ✅ Solide |
| TxBuffer | 7 Tests + 2 Proptest-Properties + Concurrency-Test | ✅ Gut |
| ResourceTracker | 6 Tests, inkl. Concurrency + Underflow/Overflow | ✅ Gut |
| Distance Metrics | 6 Tests, inkl. u8⟷f32 Ranking-Cross-Check | ✅ Gut |
| **Integration** | 2 Tests — **nur Happy-Path** | ❌ **Bluff** |
| **Fehlende Tests** | Kein Crash-Mid-Write, kein Concurrent reap+stage, kein Double-Pin/Unpin | ❌ **Lücke** |

> [!WARNING]
> Die Unit-Tests sind überdurchschnittlich gut (Proptest!). Aber die **Integration-Tests** sind reine Schönwetter-Pfade. Es fehlen:
> - Concurrent [stage()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#119-132) + [reap_orphans()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#180-196) Interleaving
> - [pin(X)](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#67-74) + [register(X)](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#48-60) + [unpin(X)](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#75-79) → Guard-Drop Sequence
> - [scan_prefix_at](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#124-132) mit aktivem Writer auf selber Prefix
