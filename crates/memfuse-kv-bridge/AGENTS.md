# memfuse-kv-bridge

**Layer**: Layer 2 (KV Cache Security Layer)

## Zweck
In-memory KV-Cache Security Layer für Tensor/KV-Segmentverwaltung. Bietet Tenant-Isolation, automatische Speicherbereinigung via Zeroize-on-Drop (`KvSegment`) und getrennte Eviction-Pfade (Worker vs. Synchronous Emergency Wipe).

## Invarianten
1. **P9 - Zeroize On Drop**: `KvSegment` muss bei Drop alle enthaltenen Tensor-Bytes sicher im Speicher überschreiben (via `zeroize::ZeroizeOnDrop`).
2. **Tenant Isolation**: Ein Tenant darf niemals Segmente eines anderen Tenants lesen (`TenantIsolatedKvStore`).
3. **P10 - Unsafe Scope**: Production-Code ist `forbid(unsafe_code)`. Test-Code nutzt `ManuallyDrop` und explizite Pointer-Inspektion ausschließlich für Zeroize-Verifikation (analoge Sicherheitsgarantie wie `memfuse-crypto/src/anti_tamper.rs`).
4. **Eviction Architecture**: H2-Sprint Architecture — Der reguläre Eviction-Pfad (`EvictionWorker`) läuft in einem dedizierten OS-Thread und blockiert nicht den Async-Executor. Der `emergency_wipe()`-Pfad arbeitet synchron und garantiert die vollständige Speichersäuberung vor der Rückkehr.
