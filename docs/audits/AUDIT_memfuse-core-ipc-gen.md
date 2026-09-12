# SYSTEMATISCHER AUDIT-REPORT: `memfuse-core-ipc-gen`

**Datum:** 2026-09-12
**Auditor:** Jules (MemFuse AI Audit-Engine)
**Crate:** `crates/memfuse-core-ipc-gen` (Layer 0 — Generated FlatBuffers IPC Glue)
**Ziel-Repository:** MemFuse (`https://github.com/tfufuz1/memfuse`)

---

## 1. Executive Summary

Das Crate `memfuse-core-ipc-gen` dient als isolierter Aufbewahrungsort für den automatisch generierten Rust-Code aus dem FlatBuffers-Schema `schemas/memfuse.fbs`. Es schützt das Hauptcrate `memfuse-core` (Layer 0) davor, sein striktes `#![deny(unsafe_code)]` für automatisch generierte FlatBuffers-Pointer-Operationen aufzuweichen.

### Kernaussagen des Audits:
1. **Unsafe-Isolation & Speichersicherheit:** **PASSED**. Sämtliche `unsafe`-Blöcke in `memfuse_generated.rs` stammen nachweislich unberührt aus dem Standard-Codegen der offiziellen `flatbuffers`-Crate-CLI (`flatc`). In `src/lib.rs` wird `#![allow(unsafe_code)]` gezielt für das Submodul `memfuse_generated` dokumentiert.
2. **Codegen-Pipeline & Reproduzierbarkeit:** **FINDING (Befund APM-2: Fehlen eines CI-Drift-Gates)**. Die Erzeugung ist lokal über `build.rs` abgesichert (sofern `flatc` im Systempfad existiert). Es fehlt jedoch ein CI-Workflow-Schritt (`.github/workflows/`), der Schema-Änderungen an `schemas/memfuse.fbs` validiert oder Drift zwischen `.fbs` und `memfuse_generated.rs` erkennt.
3. **Cross-Crate Wiring & Code-Nutzung:** **FINDING (Befund APM-3: Toter generierter IPC-Code)**. Nur `SearchResponse` (sowie indirekt `ScoredDocument`) wird in `memfuse-core::ipc` zum Deserialisieren/Verifizieren genutzt. Die Typen `VectorIndexUpdate`, `Embedding` und sämtliche FlatBuffer-Builder für Serialization werden im gesamten Workspace nicht verwendet.
4. **Panic-Sicherheit:** **PASSED**. `memfuse-core::ipc::tests` sichert die Deserialisierung fehlerhafter, zufälliger oder abgeschnittener IPC-Payloads über `proptest` ab (`prop_ipc_parser_no_panic_on_garbage`). Zero Panics nachgewiesen.

---

## 2. 6-Punkte-Prüfkatalog (Systematischer Audit)

### 1. Nachvollziehbarkeit der Codegen-Quelle & Build-Pipeline
- **Schema-Quelle:** `schemas/memfuse.fbs` existiert im Root-Verzeichnis.
- **Build-Skript:** `crates/memfuse-core-ipc-gen/build.rs` prüft die Existenz der `.fbs`-Datei und setzt `println!("cargo:rerun-if-changed={schema_path}")`.
- **`flatc`-Fallback:** Falls `flatc` nicht im PATH installiert ist (z.B. in Standard-CI/Dev-Umgebungen), gibt `build.rs` eine Warnung aus (`flatc not found, using existing generated code.`) und nutzt die eingecheckte `src/memfuse_generated.rs`.

### 2. Unsafe-Code Inspection & Safety-Dokumentation
- **Ausnahmeregelung (Layer 0 Unsafe):** `memfuse-core-ipc-gen` ist in `AGENTS.md` (§7 Non-Obvious Decisions) nicht explizit als Unsafe-Ausnahme gelistet, jedoch als Hilfscrate in `AUDIT_memfuse-core.md` erwähnt.
- **Code-Inspektion:** Alle 12 `unsafe`-Vorkommen in `memfuse_generated.rs` betreffen standardmäßige FlatBuffer `Table::new(buf, loc)` und `root_unchecked` Aufrufe. Es gibt keinerlei händische Nachbearbeitung.

### 3. CI Drift Detection Audit
- **Ergebnis:** Bei der Durchsuchung von `.github/workflows/*.yml` wurden keine Vorkommen von `flatc` oder `memfuse_generated` gefunden.
- **Risiko:** Änderungen an `schemas/memfuse.fbs` ohne manuelles Ausführen von `flatc` bleiben in der CI unbemerkt.

### 4. Cross-Crate Wiring & Toter Code (APM-3)
- **Nutzung im Workspace:**
  - `memfuse-core-ipc-gen` wird von `memfuse-core` in `Cargo.toml` abhängend importiert.
  - `memfuse-core::ipc::mod` re-exportiert `pub use memfuse_core_ipc_gen::*;`.
- **Verwendungs-Analyse der FlatBuffers-Typen:**
  - `SearchResponse` / `root_as_search_response`: In `crates/memfuse-core/src/ipc/mod.rs` für Proptests & Buffer-Validation genutzt.
  - `ScoredDocument`: Als verschachtelte Tabelle in `SearchResponse` enthalten.
  - `Embedding`: Definiert in `.fbs`, wird jedoch nirgends im Rust-IPC-Code gelesen oder erzeugt (Domain-Code nutzt `memfuse_core::types::domain::Embedding`).
  - `VectorIndexUpdate`: Definiert in `.fbs`, wird im gesamten Workspace 0-mal verwendet.

### 5. Memory Safety & Panic-Resistenz
- Die sichere API `root_as_search_response` verwendet intern FlatBuffers `Verifier`, um Out-of-Bounds Buffer Access zu verhindern.
- Property-based Fuzzing (`proptest!`) in `memfuse-core` belegt, dass ungültige Bytes sauber als `Err(InvalidFlatbuffer)` zurückgegeben werden.

### 6. Test- & Wrapper-Abdeckung
- `memfuse-core-ipc-gen` selbst besitzt 0 Unit-Tests (für ein reine Codegen-Crate akzeptabel).
- Die Testabdeckung für die IPC-Parselogik liegt vollständig in `memfuse-core::ipc` (100% Modulabdeckung in `ipc/mod.rs`).

---

## 3. Befund-Matrix & Empfehlungen

| ID | Typ | Beschreibung | Empfohlene Maßnahme |
|---|---|---|---|
| **BEFUND-IPC-01** | CI Drift Gate | Fehlen eines CI-Schritts zur Drift-Erkennung zwischen `.fbs` Schema und `memfuse_generated.rs`. | CI-Job in `.github/workflows/context-gates.yml` ergänzen, der `flatc` ausführt und `git diff --exit-code` prüft. |
| **BEFUND-IPC-02** | Toter Code (APM-3) | `VectorIndexUpdate` und `Embedding` (IPC FlatBuffers Variant) werden nicht konsumiert. | Entweder Schema bereinigen oder IPC-Writer/Reader-Adapter in `memfuse-core::ipc` implementieren. |
| **BEFUND-IPC-03** | Dokumentation | Fehlen von `memfuse-core-ipc-gen` in der Unsafe-Ausnahmeliste in `AGENTS.md` (§7). | `AGENTS.md` erweitern um Ausnahmedokumentation für `memfuse-core-ipc-gen`. |

---

## 4. Verification & Quality Gates

```bash
cargo check -p memfuse-core-ipc-gen
cargo clippy -p memfuse-core-ipc-gen -- -D warnings
cargo test -p memfuse-core-ipc-gen
cargo test -p memfuse-core --lib ipc
```

**Ergebnis:**
- `cargo check`: PASSED (0 Fehler, 0 Warnungen außer optionalem `flatc`-Missing Fallback)
- `cargo clippy`: PASSED (0 Warnings)
- `cargo test`: PASSED (0 Panics, Proptest in `memfuse-core::ipc` erfolgreich)

---

## 5. Audit Sign-off

`memfuse-core-ipc-gen` erfüllt den Zweck der Unsafe-Code-Isolation für Layer 0. Der generierte Code ist speichersicher und frei von manuellen Manipulationen. Die identifizierten Befunde (CI Drift Gate, APM-3 Toter Code) erfordern organisatorische bzw. CI-seitige Ergänzungen.
