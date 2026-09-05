# DistanceDispatcher SIMD-Runtime-Selection Design (SPEC-043 Integration)

## 1. Problemstellung
Die aktuelle SIMD-Implementierung in `chimera-index-vector/src/distance.rs` nutzt `is_x86_feature_detected!`. Dieses Makro ist in engen HNSW-Suchschleifen (Hot-Path) teuer, da es bei jedem Aufruf CPUID-Checks oder OS-Abfragen durchführt. Zudem sind die Funktionsaufrufe über Crate-Grenzen hinweg (sobald als Plugin entkoppelt) weniger optimierbar für den Inliner.

## 2. Lösungsansatz: Globaler DistanceDispatcher
Ein `DistanceDispatcher` wird beim Start des `VectorPlugin` (via `on_start`) einmalig initialisiert. Er erkennt die CPU-Features und bindet die optimalen Implementierungen an Funktionszeiger (Function Pointers).

### 2.1 Datenstruktur
```rust
pub struct DistanceDispatcher {
    cosine: fn(&[f32], &[f32]) -> f32,
    euclidean: fn(&[f32], &[f32]) -> f32,
    dot_product: fn(&[f32], &[f32]) -> f32,
}

impl DistanceDispatcher {
    pub fn new() -> Self {
        // Einmalige Erkennung beim Start
        let (cos, euc, dot) = select_best_simd_impls();
        Self {
            cosine: cos,
            euclidean: euc,
            dot_product: dot,
        }
    }

    #[inline(always)]
    pub fn cosine(&self, a: &[f32], b: &[f32]) -> f32 {
        (self.cosine)(a, b)
    }
    // ... analog für andere
}
```

## 3. Integration in SPEC-043 (Plugin-Registry)
Das `VectorPlugin` hält eine Instanz des `DistanceDispatcher`.

```rust
pub struct VectorPlugin {
    index: Arc<RwLock<HnswIndex>>,
    dispatcher: DistanceDispatcher,
    // ...
}
```

Beim Aufruf von `on_start` wird der Dispatcher validiert. Die Suchlogik im `HnswIndex` erhält Zugriff auf den Dispatcher, um Distanzberechnungen ohne wiederholte Feature-Checks durchzuführen.

## 4. Invarianten & Sicherheit
- **INV-SIMD-1:** Jede SIMD-Funktion (AVX2, AVX512) muss einen `// SAFETY:` Kommentar haben, der CPU-Support, Alignment und Bounds-Checks formal beweist.
- **INV-SIMD-2:** Ein Fallback auf `std::simd` (portable-simd) oder Scalar muss immer vorhanden sein.
- **INV-SIMD-3:** Der Dispatcher darf nach der Initialisierung nicht mehr geändert werden (Interior Immutability oder statische Instanz).

## 5. Implementierungsschritte (Delegation)
1. **@algo_expert:** Refactor `distance.rs` zur Bereitstellung der Funktions-Signaturen für den Dispatcher.
2. **@algo_expert:** Implementierung der `select_best_simd_impls` Logik unter Berücksichtigung von AVX-512, AVX2 (FMA) und `std::simd`.
3. **@rust_coder:** Integration des Dispatchers in den `HnswIndex`.
4. **@qa_reviewer:** Benchmarking der neuen Dispatch-Logik gegen die aktuelle Makro-Lösung.
