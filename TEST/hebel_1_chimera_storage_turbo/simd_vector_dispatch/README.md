# SIMD-Beschleunigung (`portable-simd` + AVX-512) — Integration Guide für MemFuse

## 1. Technischer Hintergrund & Synergie
In MemFuse (`memfuse-core` und `memfuse-db`) werden Vektordistanzberechnungen (Cosine Similarity, Euklidische Distanz, Dot Product) bei jedem HNSW-Graph-Traversal und Flat-Scan millionenfach pro Sekunde ausgeführt. Standard-Skalarschleifen oder Auto-Vektorisierung durch LLVM garantieren keine optimale Ausnutzung von 256-Bit (AVX2) oder 512-Bit (AVX-512) Registern, insbesondere bei variablen Laufzeit-CPU-Features.

**Project Chimera** hat dieses Problem mit einer Multi-Tier-Architektur gelöst:
1. **Tier 1: AVX-512 Intrinsics** (`_mm512_loadu_ps`, `_mm512_fmadd_ps`, `_mm512_reduce_add_ps`): Verarbeitet 16 Floats pro Taktzyklus mit Fused Multiply-Add (FMA).
2. **Tier 2: AVX2 Intrinsics** (`_mm256_loadu_ps`, `_mm256_fmadd_ps`): Verarbeitet 8 Floats pro Taktzyklus mit 32-Byte Alignment Support.
3. **Tier 3: `std::simd` (`portable-simd`)**: Portabler SIMD-Fallback für ARM NEON und x86 ohne explizite Unsafe-Intrinsics.
4. **Tier 4: Scalar Fallback**: Garantiert Ausführbarkeit auf jeder Zielhardware.

## 2. Extrahierte Chimera-Komponenten

| Datei | Quelle | Relevanz für MemFuse |
|:---|:---|:---|
| [`distance.rs`](./distance.rs) | `chimera-index-vector/src/distance.rs` | Vollständige SIMD-Implementierung mit Runtime Feature Detection (`is_x86_feature_detected!`) |
| [`SPEC-001_simd_distance.md`](./SPEC-001_simd_distance.md) | `docs/specs/SPEC-001_simd_distance.md` | Formale Spezifikation & Invarianten für SIMD Vektordistanz |
| [`ADR-011_distance_dispatcher.md`](./ADR-011_distance_dispatcher.md) | `docs/architecture/ADR-011_distance_dispatcher.md` | Architecture Decision Record: O(1) Startup-Dispatcher ohne CPUID-Check im Hot-Path |
| [`distance_bench.rs`](./distance_bench.rs) | `chimera-index-vector/benches/distance_bench.rs` | Criterion Benchmarks zum Nachweis des ≥ 4x–8x Speedups gegenüber Scalar |

## 3. Kern-Code-Auszug: SIMD Cosine Distance
Aus [`distance.rs`](./distance.rs):
```rust
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector lengths must be equal");
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let a_ptr = a.as_ptr() as usize;
        let b_ptr = b.as_ptr() as usize;

        // Try AVX-512 first for maximum performance
        if is_x86_feature_detected!("avx512f")
            && a_ptr.is_multiple_of(64)
            && b_ptr.is_multiple_of(64)
        {
            return unsafe { cosine_distance_avx512(a, b) };
        }
        // Then AVX2 if AVX-512 is not available or not aligned
        if is_x86_feature_detected!("avx2") && a_ptr.is_multiple_of(32) && b_ptr.is_multiple_of(32)
        {
            return unsafe { cosine_distance_avx2(a, b) };
        }
    }
    // Portable-simd as default high-performance fallback
    cosine_distance_std_simd(a, b)
}
```

## 4. Implementierungsplan für MemFuse
1. Kopiere `distance.rs` in `crates/memfuse-core/src/simd_distance.rs`.
2. Verbinde `compute_distance()` mit den HNSW-Index-Traversierungs-Methoden in `memfuse-db`.
3. Validiere mit `cargo test --release` und führe den beiliegenden `distance_bench.rs` Benchmark aus.
