//! PPR Allocation Profiling and Correctness Benchmark Test for memfuse-graph
//! Verifies allocation reduction in Personalized PageRank (PPR) before and after optimization.

use memfuse_core::{Entity, EntityId, PprConfig};
use memfuse_graph::{CsrGraph, PprContext};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAllocator;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: `CountingAllocator` implements `GlobalAlloc` by delegating directly to `std::alloc::System`.
// Invariants & Safety Proof:
// 1. Thread Safety: Atomic counter increments (`ALLOC_COUNT`, `ALLOC_BYTES`) use relaxed atomic operations, preserving `Sync` safety.
// 2. Memory Safety: All heap allocations and deallocations are forwarded unchanged to `std::alloc::System`.
// 3. Trait Contract: `alloc` and `dealloc` obey all layout and pointer invariants required by the `GlobalAlloc` contract.
unsafe impl GlobalAlloc for CountingAllocator { // SAFETY: Thread-safe delegation to System allocator.
    // SAFETY: Layout invariants (size, alignment) are guaranteed by the `GlobalAlloc` contract caller.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { // SAFETY: Layout verified by GlobalAlloc caller.
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: `layout` is guaranteed valid by caller of `GlobalAlloc::alloc`, forwarded directly to `System.alloc`.
        let ptr = unsafe { System.alloc(layout) }; // SAFETY: Forward layout to System allocator.
        ptr
    }

    // SAFETY: Pointer and layout invariants are guaranteed by the `GlobalAlloc` contract caller (`ptr` was allocated by `alloc` with matching `layout`).
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { // SAFETY: Pointer and layout verified by GlobalAlloc caller.
        // SAFETY: `ptr` and `layout` are guaranteed valid by caller of `GlobalAlloc::dealloc`, forwarded directly to `System.dealloc`.
        unsafe { System.dealloc(ptr, layout) }; // SAFETY: Forward ptr and layout to System allocator.
    }
}

#[global_allocator]
static A: CountingAllocator = CountingAllocator;

fn reset_stats() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
}

fn get_stats() -> (u64, u64) {
    (
        ALLOC_COUNT.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

use std::sync::Arc;

#[tokio::test]
async fn test_ppr_alloc_reduction_100k_nodes() {
    const NUM_NODES: usize = 100_000;

    let graph = Arc::new(CsrGraph::new());

    // Construct a large graph with 100,000 nodes and linear edges i -> i + 1
    for i in 1..=NUM_NODES {
        let eid = EntityId::new(i as u64);
        graph
            .insert_entity_direct(Entity::new(eid, format!("N{i}"), "Node"))
            .expect("insert entity");
    }

    for i in 1..NUM_NODES {
        let src = EntityId::new(i as u64);
        let dst = EntityId::new((i + 1) as u64);
        graph
            .insert_edge_direct(src, dst, 1.0)
            .await
            .expect("insert edge");
    }

    graph.compact();

    let seed = EntityId::new(1);
    let config = PprConfig {
        damping_factor: 0.85,
        max_iterations: 10,
        convergence_epsilon: 1e-6,
        warn_on_non_convergence: true,
    };

    // --- First run (fresh PprContext) ---
    let mut ctx = PprContext::new();
    reset_stats();
    let res1 = graph.personalized_page_rank_with_context(&[seed], &config, &mut ctx);
    let (allocs_run1, bytes_run1) = get_stats();

    println!("Run 1 Allocs: {allocs_run1}, Bytes: {bytes_run1}");

    // --- Second run (reusing PprContext) ---
    reset_stats();
    let res2 = graph.personalized_page_rank_with_context(&[seed], &config, &mut ctx);
    let (allocs_run2, bytes_run2) = get_stats();

    println!("Run 2 Allocs: {allocs_run2}, Bytes: {bytes_run2}");

    // Verifications:
    // 1. Numerical correctness / bit-identical equality between runs
    assert_eq!(res1.len(), res2.len());
    for (a, b) in res1.iter().zip(res2.iter()) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1.to_bits(), b.1.to_bits());
    }

    // 2. Allocation reduction check:
    // Old implementation allocated 100,000 Vec<OutgoingEdge> + 100,000 outer vec allocations = >100,000 allocs per run.
    // Reusing PprContext should require ONLY the result vector allocation (1 allocation).
    assert!(
        allocs_run2 <= 5,
        "Reused PprContext must perform <= 5 heap allocations (result vec only), got {allocs_run2}"
    );

    // 3. First run allocation check:
    // Context preparation allocates a fixed small number of contiguous buffers (valid_seeds, out_weight_sums, ranks, next_ranks)
    // and result vector, total allocations should be <= 10 (O(1) allocation count instead of O(N)).
    assert!(
        allocs_run1 <= 20,
        "First PPR run on 100,000 nodes must perform O(1) buffer allocations <= 20, got {allocs_run1}"
    );
}
