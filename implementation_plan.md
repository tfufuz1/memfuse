# Goal Description
We need to address the three primary blind spots identified in the "MemFuse Codebase Analysis & Blind Spot Report" to ensure production-grade stability (Zero-Panic, Sovereign Core Doctrine).

1. **Safe CPU Feature Detection:** Ensure rigorous CPU feature detection before calling AVX2/AVX-512 unsafe functions to prevent SIGILL faults.
2. **Cancelable Task Management:** Replace detached `tokio::spawn` calls with cancellation-aware task groups (`CancellationToken` + Graceful Shutdown).
3. **Enforce Trait Generics / Remove async_trait:** Remove `async_trait` from core interfaces (e.g., `StorageEngine`) and switch to statically-dispatched generics (`<S: StorageEngine>`) instead of dynamic dispatch (`Box<dyn StorageEngine>`) to eliminate Box allocation latency.

## Proposed Changes

### `memfuse-index`
#### [MODIFY] [distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs)
- Update standard CPU feature guards: e.g. before calling [dot_product_u8_avx512vnni](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs#701-730), verify both `avx512f` and [avx512vnni](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs#701-730) are present. 
- Ensure all AVX paths fall back to `*_scalar` securely.

### `memfuse-store` & `memfuse-db` (Task Management)
- Introduce `tokio_util::sync::CancellationToken` and potentially a WaitGroup/Tracker for safe shutdown.
- Refactor all background jobs currently spawned via `tokio::spawn` (e.g., compaction, checkpointing, reaper) to accept a `cancel_token: CancellationToken`.
- Wrap the main engine loops in `tokio::select!` so they drop correctly when the token is canceled.

#### [MODIFY] [lsm.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs)
- Refactor `CompactionEngine` spawning.
#### [MODIFY] [compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs)
- Introduce token to prevent task leaking.
#### [MODIFY] [checkpoint.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/checkpoint.rs)
- Use tokens.
#### [MODIFY] [reaper.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/reaper.rs)
- Use tokens.

### `memfuse-core` & Orchestration (Trait Generics)
#### [MODIFY] [traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs)
- Remove `#[async_trait]` from `StorageEngine` and `IndexEngine`. 
- Leverage native Rust `async fn` in traits (AFIT) or return `impl Future`.

#### [MODIFY] Engine Crates (e.g., `memfuse-db`)
- Switch `dyn StorageEngine` to generics like `<S: StorageEngine>`.
- Remove `Box<...>` usage for trait objects on the hot path.

## Verification Plan

### Automated Tests
- Run `cargo check -p memfuse-core -p memfuse-index -p memfuse-store -p memfuse-db` to verify the generic trait transition and new async boundaries.
- Run `just triple-test` per the AGENTS.md requirements to validate no functionality broke during refactor.

### Manual Verification
- Review task startup/shutdown traces to ensure `CancellationToken` fires cleanly.
- Verify SIGILL avoidance via CPU target masks if necessary (though simple cargo checks validate the logic paths).
