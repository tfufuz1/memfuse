// FILE-CONTEXT
// ZWECK: KV-Cache-Bridge Sicherheitsschicht (KvSegment, Tenant-Isolation, Eviction-Worker, Segment-Verschlüsselung).
// STAND: TS:2026-09-08T00:00:00Z (SESSION: a413a598)

pub mod eviction_worker;
pub mod segment;
pub mod store;

pub use eviction_worker::{emergency_wipe, EvictionWorker};
pub use segment::{KvSegment, CURRENT_KV_KEY_DERIVATION_VERSION};
pub use store::TenantIsolatedKvStore;
