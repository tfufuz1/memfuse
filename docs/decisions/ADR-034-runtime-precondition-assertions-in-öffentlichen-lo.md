# ADR-034: Runtime-Precondition Assertions in öffentlichen Low-Level-Distanzfunktionen (`memfuse-index`)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Behebung von Befund F-08 (`AGT-INDEX-005`). Die low-level Distanzfunktionen `cosine_distance`, `euclidean_distance` und `dot_product_distance` in `memfuse-index/src/distance.rs` sind `pub` exportiert. Bisher schützten sie Slice-Längengleichheiten nur via `debug_assert_eq!`, was in Release-Builds (`opt-level = 3`, LTO) entfernt wurde. Bei fehlerhaften Aufrufen mit ungleichen Slice-Längen drohte in den nachfolgenden `unsafe`-SIMD-Blöcken (AVX2/AVX512/NEON) ein stummer Out-of-Bounds Buffer-Overread (Undefined Behavior).
*   **Entscheidung**:
    - Ersetzung von `debug_assert_eq!(a.len(), b.len())` durch eine release-aktive Laufzeitprüfung `assert_eq!(a.len(), b.len(), "Vector lengths must match")` in allen drei öffentlichen Distanzfunktionen.
    - Dokumentation der Vorbedingung und des Panic-Vertrags in einer expliziten Rustdoc `/// # Panics` Sektion an jeder Funktion.
    - Autorisierung dieser Panic-Prüfung als explizit dokumentierte Ausnahme von der "No Panics in libraries"-Doktrin (CONSTITUTION.md), da es sich um die Durchsetzung von Verträgen bei low-level SIMD-Funktionen handelt, deren Signatur (`-> f32`) für Hot-Path-Performance erhalten bleiben muss.
*   **Alternativen**:
    - **Option A (Signaturänderung zu `-> Result<f32, ...>`)**: Verworfen, da dies signifikanten Overhead auf dem Hot-Path erzeugen und alle Aufrufer sowie Benchmarks brechen würde.
    - **Option B (Sichtbarkeit auf `pub(crate)` reduzieren)**: Verworfen/abgewogen gegen Option 1, da `cosine_distance`, `euclidean_distance` und `dot_product_distance` als public Utility-API des `memfuse-index`-Crates etabliert sind und in Benchmarks/Tests genutzt werden.
*   **Begründung**: Der O(1) Längen-Check ist gegenüber der O(n) SIMD-Berechnung vernachlässigbar. Die explizite Panic bei Vorbedingungsverletzung schützt zu 100% vor Undefined Behavior und Memory-Safety-Verstößen an den `unsafe` SIMD-Grenzen.

---
