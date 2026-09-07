// FILE-CONTEXT
// ZWECK: KV-Cache-Bridge Sicherheitsschicht (KvSegment, Tenant-Isolation, Eviction-Worker).
// STAND: TS:2026-09-07T12:00:00Z (SESSION: a413a598)

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod eviction_worker;
pub mod segment;
pub mod store;

pub use eviction_worker::{emergency_wipe, EvictionWorker};
pub use segment::KvSegment;
pub use store::TenantIsolatedKvStore;
