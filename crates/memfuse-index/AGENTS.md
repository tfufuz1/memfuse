# memfuse-index — Agent Context

## 🎯 Crate Purpose
`memfuse-index` ist der mathematische und algorithmische Kern der Vektordatenbank. Es implementiert den **HNSW Vector Index** (Hierarchical Navigable Small World) und hardware-beschleunigte SIMD-Distanzmetriken (AVX-512, NEON).

## 🛡️ Critical Invariants
- **[INV-MATH-1] `unsafe` für SIMD**: `unsafe` Blöcke sind hier erlaubt, **aber**: Jeder Block erfordert zwingend das Kommentarformat `// SAFETY: [Kurzer Beweis der Speichersicherheit]`.
- **[INV-MATH-2] NaN/Inf Protection**: Alle Distanzberechnungen müssen Infinity oder `NaN` Vectors direkt beim Check ablehnen. Keine Vergiftung des Index.
- **[INV-NAV-1] Graph Connectivity**: Der HNSW-Graph muss immer zusammenhängend bleiben. Implementiere Check-Routinen in Tests, um Graph-Zersplitterungen (Disconnected Components) nach Löschungen auszuschließen.

## 🔄 TDD Workflow Requirement
Alle mathematischen Operationen müssen über Micro-Tests abgedeckt werden.
1. Bevor du eine neue Heuristik für das Graph-Routing oder eine SIMD-Operation baust, schreibe einen deterministischen Benchmark- oder Unit-Test mit definierten festen Vector-Werten.
2. Der Test muss extrem schnell sein (<10ms).
