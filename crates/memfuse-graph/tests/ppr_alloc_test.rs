//! PPR Allocation Profiling and Correctness Benchmark Test for memfuse-graph
//! Verifies allocation reduction in Personalized PageRank (PPR) before and after optimization.

use memfuse_core::{Entity, EntityId, PprConfig};
use memfuse_graph::{CsrGraph, PprContext};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAllocator;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: CountingAllocator forwards all memory allocation requests directly to `System.alloc`/`System.dealloc`
// while safely recording allocation counts and byte sizes via atomic counters.
unsafe impl GlobalAlloc for CountingAllocator {
    // SAFETY: Invariant: `layout` is valid and non-zero size as required by `System.alloc`.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: Delegated directly to System allocator with caller's valid layout.
        unsafe { System.alloc(layout) }
    }

    // SAFETY: Invariant: `ptr` was previously allocated by `System.alloc` with matching `layout`.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Delegated directly to System deallocator with valid ptr and layout.
        unsafe { System.dealloc(ptr, layout) };
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
