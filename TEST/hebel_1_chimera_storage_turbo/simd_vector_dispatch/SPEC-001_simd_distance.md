# SPEC-001: Portable SIMD Distance Functions

> **Priorität:** 🔴 Hoch | **Crate:** `chimera-index-vector` | **Abhängigkeit:** Keine

## 1. Problem

Die aktuellen Distance-Funktionen in [`distance.rs`](file:///home/freddy/Arbeitsplatz/DEV/chimeraDB/crates/chimera-index-vector/src/distance.rs) delegieren an `simsimd` (C-FFI). Der MASTER_ROADMAP verlangt native `portable-simd` Integration für:
- Eliminierung des C-FFI Overheads
- Nutzung von Rust's `#![feature(portable_simd)]` für plattformübergreifende SIMD
- Fallback auf Auto-Vektorisierung (kein Laufzeit-Dispatch nötig)

## 2. IST-Zustand

```rust
// distance.rs (aktuell)
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if let Some(res) = simsimd::SpatialSimilarity::cosine(a, b) {
        res as f32
    } else {
        cosine_distance_scalar(a, b)
    }
}
```

Scalar-Fallbacks sind doppelt definiert (einmal in `#[cfg(test)]`, einmal öffentlich).

## 3. SOLL-Zustand

### 3.1 Dateistruktur

```
chimera-index-vector/src/
├── distance.rs          # Dispatch-Logik + Trait
├── distance/
│   ├── mod.rs           # Re-exports
│   ├── scalar.rs        # Referenz-Implementierungen
│   ├── simd_portable.rs # portable-simd Implementierungen
│   └── bench.rs         # Inline-Benchmarks
```

### 3.2 API-Kontrakt

```rust
/// Trait für austauschbare Distance-Backends.
pub trait DistanceCompute: Send + Sync {
    fn cosine(&self, a: &[f32], b: &[f32]) -> f32;
    fn euclidean(&self, a: &[f32], b: &[f32]) -> f32;
    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32;
}

/// SIMD-beschleunigte Implementierung via portable-simd.
pub struct PortableSimdDistance;

/// Scalar-Fallback (Referenz für Tests).
pub struct ScalarDistance;

/// Compile-time Feature-Gate:
/// - `simd` Feature → PortableSimdDistance
/// - Default → ScalarDistance
pub fn default_distance_backend() -> Box<dyn DistanceCompute> { ... }
```

### 3.3 Portable-SIMD Implementierung

```rust
// simd_portable.rs
#![feature(portable_simd)]
use std::simd::{f32x16, SimdFloat, StdFloat};

impl DistanceCompute for PortableSimdDistance {
    fn cosine(&self, a: &[f32], b: &[f32]) -> f32 {
        let (chunks_a, remainder_a) = a.as_chunks::<16>();
        let (chunks_b, _) = b.as_chunks::<16>();

        let mut dot = f32x16::splat(0.0);
        let mut norm_a = f32x16::splat(0.0);
        let mut norm_b = f32x16::splat(0.0);

        for (ca, cb) in chunks_a.iter().zip(chunks_b) {
            let va = f32x16::from_slice(ca);
            let vb = f32x16::from_slice(cb);
            dot += va * vb;
            norm_a += va * va;
            norm_b += vb * vb;
        }

        let dot_sum = dot.reduce_sum() + scalar_dot(remainder_a, &b[a.len() - remainder_a.len()..]);
        let na = norm_a.reduce_sum().sqrt();
        let nb = norm_b.reduce_sum().sqrt();

        if na == 0.0 || nb == 0.0 { 1.0 } else { 1.0 - (dot_sum / (na * nb)) }
    }
    // Analog für euclidean und dot_product
}
```

### 3.4 Cargo.toml Änderungen

```toml
[features]
default = []
simd = [] # Aktiviert portable-simd Backend

[dependencies]
# simsimd bleibt als optionale Dependency für Benchmarking-Vergleich
simsimd = { version = "...", optional = true }
```

## 4. Migrationsstrategie

1. **Neue Dateien anlegen** (`distance/` Modul)
2. **`DistanceCompute` Trait** definieren
3. **Scalar-Backend** implementieren (aus doppeltem Code konsolidieren)
4. **Portable-SIMD Backend** implementieren (hinter `#[cfg(feature = "simd")]`)
5. **`HNSWIndex`** refactoren: `distance_fn` Feld statt direktem `compute_distance` Call
6. **`simsimd`** Abhängigkeit als `optional` markieren
7. **Benchmarks** erweitern: `scalar` vs `simsimd` vs `portable-simd`

## 5. Tests

```rust
#[test]
fn simd_matches_scalar_cosine() {
    let a: Vec<f32> = (0..1536).map(|i| (i as f32) * 0.001).collect();
    let b: Vec<f32> = (0..1536).map(|i| (1536 - i) as f32 * 0.001).collect();

    let scalar = ScalarDistance.cosine(&a, &b);
    let simd = PortableSimdDistance.cosine(&a, &b);
    assert!((scalar - simd).abs() < 1e-5);
}

// Proptest: Beliebige Vektoren gleicher Dimension
proptest! {
    #[test]
    fn prop_simd_euclidean_matches_scalar(dim in 16..2048usize) {
        // Generiere Random-Vektoren, vergleiche Ergebnisse
    }
}
```

## 6. Akzeptanzkriterien

- [ ] `cargo test` grün mit und ohne `simd` Feature
- [ ] `cargo bench` zeigt ≥2x Speedup gegenüber Scalar auf AVX2-Hardware
- [ ] Keine `unsafe` Blöcke (portable-simd ist safe)
- [ ] `simsimd` Dependency ist `optional`
- [ ] Kein doppelter Scalar-Code mehr
