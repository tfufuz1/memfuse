# AGENTS.md — memfuse-kv-bridge
> Layer 1 | KV-Cache Security Layer (KvSegment, Tenant Isolation, Eviction Worker) | ~250 LOC

## 1. Zweck & Architekturrolle

In-memory KV-Cache Security Layer für Tensor/KV-Segmentverwaltung. Bietet Tenant-Isolation, automatische Speicherbereinigung via Zeroize-on-Drop (`KvSegment`) und getrennte Eviction-Pfade (Worker vs. Synchronous Emergency Wipe).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklaration, `#![cfg_attr(not(test), forbid(unsafe_code))]` |
| `segment.rs` | `KvSegment` — Tensor-Segment mit `#[derive(Zeroize, ZeroizeOnDrop)]`, Redacted Debug |
| `eviction_worker.rs` | `EvictionWorker` (dedizierter OS-Thread für non-blocking LRU) und `emergency_wipe()` |
| `store.rs` | `TenantIsolatedKvStore` — Tenant-partitionierter KV-Store (`INV-TENANT` Isolation) |

## 3. Kritische Invarianten

### P9 - Zeroize On Drop
`KvSegment` muss bei Drop alle enthaltenen Tensor-Bytes sicher im Speicher überschreiben (via `zeroize::ZeroizeOnDrop`).

### Tenant Isolation
Ein Tenant darf niemals Segmente eines anderen Tenants lesen (`TenantIsolatedKvStore`).

### P10 - Unsafe Scope
Production-Code ist `forbid(unsafe_code)`. Test-Code nutzt `ManuallyDrop` und explizite Pointer-Inspektion ausschließlich für Zeroize-Verifikation (analoge Sicherheitsgarantie wie `memfuse-crypto/src/anti_tamper.rs`).

### Eviction Architecture
Der reguläre Eviction-Pfad (`EvictionWorker`) läuft in einem dedizierten OS-Thread und blockiert nicht den Async-Executor. Der `emergency_wipe()`-Pfad arbeitet synchron und garantiert die vollständige Speichersäuberung vor der Rückkehr.

## 4. Public API Quick-Reference

```rust
pub struct KvSegment { ... }
impl KvSegment {
    pub fn new(tenant_id: TenantId, segment_id: u64, data: Vec<u8>) -> Self;
    pub fn as_bytes(&self) -> &[u8];
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}

pub struct EvictionWorker { ... }
impl EvictionWorker {
    pub fn spawn(segments: Arc<RwLock<Vec<KvSegment>>>) -> Self;
    pub fn trigger_eviction(&self, target_free_bytes: usize);
    pub fn shutdown(&mut self);
}

pub fn emergency_wipe(segments: &RwLock<Vec<KvSegment>>);

pub struct TenantIsolatedKvStore { ... }
impl TenantIsolatedKvStore {
    pub fn new() -> Self;
    pub fn insert_segment(&self, tenant: TenantId, segment: KvSegment);
    pub fn get_segments(&self, tenant: TenantId) -> Vec<u64>;
    pub fn get_tenant_segment_len(&self, tenant: TenantId) -> usize;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Asynchrone LRU-Eviction im Tokio-Executor blockieren:
async fn evict_lru(...) { ... }

// ✅ KORREKT — Non-blocking trigger über MPSC-Channel an dedizierten OS-Thread:
worker.trigger_eviction(target_free_bytes);
```

## 6. Concurrency & Lock-Hierarchie

`TenantIsolatedKvStore` schützt interne HashMaps mit `parking_lot::RwLock`. Lock-Guards dürfen niemals über `.await`-Punkte gehalten werden.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-candle` (L3 Peer/Upper), `memfuse-db` (L2 Upper)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| P9 | Zeroize-on-Drop Security Guarantee |
| P10 | Unsafe Scope & Verification Pattern |
