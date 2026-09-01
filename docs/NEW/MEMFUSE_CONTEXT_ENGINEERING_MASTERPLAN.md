# MemFuse — Context Engineering Masterplan & Jules Refactoring Prompt
> Erstellt: 2026-08-29 | Auftraggeber: Enterprise Context Engineering  
> Basis: Repo-Analyse (301 Dateien, 15 Crates), Systemdokumente, Jules Best-Practice-Research

---

## TEIL 1 — SYSTEMANALYSE

### 1.1 Stärken des bestehenden Systems

Das MemFuse-Projekt hat bereits eine **überdurchschnittlich reife Context-Engineering-Basis**:

- **Vollständige Tag-Taxonomie** mit 4 Tag-Typen (AI-TAG, ANCHOR, REVIEW-PASS, FILE-CONTEXT), CI-enforced via `context-gates.yml` (9 Gates)
- **Multi-Session-Review-Gate** — `DONE`-ANCHORs erfordern 2–3 unabhängige Review-Sessions (Gate 8), maschinenprüfbar via `cargo xtask check-review-coverage`
- **Autogenerierte Dokumentation** — `WORKING_STATE.md`, `docs/CHANGELOG.md`, `docs/ARCHITECTURE.md` werden vollständig aus Inline-Tags generiert (`cargo xtask sync-docs`)
- **Crate-level AGENTS.md** in 13 von 15 Crates — lokales Regelwerk nah am Code
- **Anti-Halluzinations-Guards** in `.jules/COMMON_LLM_ERRORS.md` (bekannte falsche API-Signaturen, Typ-Duplikate, stille Fehler)
- **Maschinenlesbare Session-Bootstrap-Checkliste** in `.jules/SESSION_BOOTSTRAP.md` — 5-Phasen-Sequenz die Jules autonom abarbeiten kann
- **Hauptprotokoll AGENTS.md** mit 100 Zeilen — unter der empirisch ermittelten 150-Zeilen-Grenze für optimale LLM-Performance

---

### 1.2 Kritische Lücken (nach Priorität)

#### 🔴 KRITISCH — Direkte Funktionsbeeinträchtigung

**Lücke 1: `chrono_or_today()` liefert falsches Datum**
```rust
// xtask/src/main.rs:600-601 — HARDCODED, VERALTET
fn chrono_or_today() -> String {
    "2026-08-27".to_string()  // ← immer falsch nach diesem Datum
}
```
Alle autogenerierten Dokumente (`WORKING_STATE.md`, `CHANGELOG.md`) zeigen „Stand: 2026-08-27" — unabhängig vom tatsächlichen Datum. Das desorientiert Jules bei der zeitlichen Einordnung von Problemen.

**Lücke 2: 25+ große Dateien ohne FILE-CONTEXT-Header**

Dateien >50 Zeilen ohne Inline-Kontext (absteigende Priorität nach LOC):
```
2936 crates/memfuse-index/src/hnsw.rs           ← HNSW-Core, hochkomplex
2144 crates/memfuse-store/src/sstable.rs         ← SSTable-Implementierung
1817 crates/memfuse-db/src/lib.rs                ← Orchestrator-Facade
1552 crates/memfuse-ollama/src/client.rs         ← Ollama HTTP-Client
1518 crates/memfuse-text/src/inverted.rs         ← BM25-Index
1192 crates/memfuse-core/src/traits.rs           ← Alle Public Traits
1186 crates/memfuse-store/src/compaction.rs      ← STCS-Compaction
 987 crates/memfuse-py/src/lib.rs                ← Python-FFI-Grenze
 951 crates/memfuse-core/src/types/domain.rs     ← Domain-Types
 933 crates/memfuse-checkpoint/src/lib.rs        ← Checkpoint/WAL-Sync
 882 crates/memfuse-text/src/morphology.rs       ← Morphologie
 785 crates/memfuse-graph/src/ppr.rs             ← PPR-Algorithmus
 751 crates/memfuse-core/src/tx_buffer.rs        ← TxBuffer
 716 crates/memfuse-mcp/src/lib.rs               ← stdio-MCP-Server
 703 crates/memfuse-core/src/error.rs            ← Fehler-Enum
```
Jules muss bei jeder Bearbeitung ohne Kontext-Ankerpunkt auskommen — erhöhte Halluzinationsgefahr.

**Lücke 3: 2 Crates ohne AGENTS.md**
```
crates/memfuse-agent/   ← Layer-3 Workflow-Engine (7 Quelldateien)
crates/memfuse-router/  ← Layer-3 SLM-Router (5 Quelldateien)
```
Jules kann die Invarianten und Fallstricke dieser Crates nicht kennen, ohne sie aus dem Code zu rekonstruieren — fehleranfällig und zeitaufwendig.

**Lücke 4: Duplizierte ANCHOR-Einträge**

In mehreren Dateien erscheinen identische ANCHOR-Zeilen auf aufeinanderfolgenden Zeilen (z.B. `hnsw.rs`, `lsm.rs`). Das täuscht doppelte offene Arbeit vor und erzeugt Rauschen im Tag-Scan.

#### 🟡 VERBESSERUNGSWÜRDIG — Effizienzgewinne

**Lücke 5: CI Gate 7 prüft nur TS-Feld, nicht SESSION-Feld**

`context-gates.yml` Gate 7 (Zeitstempel-Compliance) grep-t nur auf `TS:[0-9]{4}-[0-9]{2}-[0-9]{2}T`, aber nicht auf `SESSION:`. Das `tag_taxonomy.md` bezeichnet fehlendes SESSION als Grammatikverstoß.

**Lücke 6: `cargo xtask` fehlt strukturierter Abfrage-Output**

Das Cheat-Sheet dokumentiert `cargo xtask context-tags --crate <X> --severity CRITICAL` mit NDJSON-Output — diese Subcommands **existieren nicht** im tatsächlichen `xtask/src/main.rs`. Jules muss auf rohe `grep`-Aufrufe ausweichen, die weniger präzise sind.

**Lücke 7: `session-context` Justfile-Befehl ist minimal**

Der `just session-context` Befehl zeigt nur offene CRITICAL-Tags und IN-PROGRESS-ANCHORs. Er zeigt nicht: letzte 3 ADRs, WORKING_STATE.md-Tail, Anzahl offener Tags pro Crate.

---

### 1.3 Systemarchitektur-Bewertung

```
                    JULES CONTEXT PIPELINE
                    
  Session-Start           Arbeit                 Session-Ende
  ┌──────────┐     ┌────────────────────┐     ┌────────────┐
  │SESSION_  │     │ crate/AGENTS.md    │     │cargo fmt   │
  │BOOTSTRAP │────>│ FILE-CONTEXT heads │────>│sync-docs   │
  │.md       │     │ AI-TAG / ANCHOR    │     │REVIEW-PASS │
  └──────────┘     │ COMMON_LLM_ERRORS  │     └────────────┘
       │           └────────────────────┘           │
       │                    │                       │
       v                    v                       v
  [PHASE 0-1]        [PHASE 2-4]           [PHASE 5 + CI]
  Identität &        Code schreiben        Gates 1-9
  Kontext laden      mit Inline-Anchoring  (context-gates.yml)
  
  QUALITÄTSBEWERTUNG:
  ✅ Tag-Taxonomie         (robust, CI-enforced)
  ✅ Multi-Session-Review  (kryptografisch überprüfbar)
  ✅ Auto-Docs             (WORKING_STATE, CHANGELOG)
  ✅ Anti-Halluzination    (COMMON_LLM_ERRORS.md)
  ⚠️  FILE-CONTEXT Coverage (25 von ~40 priority files fehlen)
  ⚠️  Crate AGENTS.md      (13 von 15 — 2 fehlen)
  ❌ chrono_or_today()     (hardcoded, immer falsch)
  ❌ xtask context-tags    (in Cheat-Sheet, aber nicht implementiert)
```

---

## TEIL 2 — VERBESSERTES SYSTEM-DESIGN

### 2.1 FILE-CONTEXT: Erweitertes Format

Das bestehende Format wird um ein `HOTSPOTS`-Feld erweitert, das Bereiche mit hoher Änderungsfrequenz markiert. Dies gibt Jules immediate Orientierung, wo Vorsicht geboten ist:

```rust
// FILE-CONTEXT
// STAND:       2026-08-29T10:00:00Z (SESSION: <8-hex>)
// ZWECK:       <Ein Satz — was diese Datei tut>
// INVARIANTEN: <Komma-separierte Must-Haves bei jeder Änderung>
// HOTSPOTS:    <Zeilenbereiche/Funktionen mit hoher Änderungsfrequenz>
// SIEHE AUCH:  <Pfade zu ADRs/rules/*.md>
// AGENT-NOTIZ: <Optional — was dieser Agent dem nächsten mitteilen will>
```

**Maximale Länge: 8 Zeilen.** Kein Ersatz für Rustdoc.

### 2.2 Neue xtask Subcommands

#### `cargo xtask context-tags` (NDJSON-Output)

Ermöglicht effizientes LLM-Parsing ohne `grep`:

```bash
# Alle CRITICAL/BLOCKER Tags als NDJSON
cargo xtask context-tags --severity CRITICAL --status OPEN

# Output (eine JSON-Zeile pro Tag):
{"file":"crates/memfuse-db/src/relate.rs","line":5,"type":"AI-TAG","cat":"CONCURRENCY","sev":"CRITICAL","id":"AGT-DB-005","ts":"2026-08-29T10:00:00Z","session":"a3f29c1d","status":"OPEN","desc":"Race condition in relate()"}
```

```bash
# Pro-Crate-Digest
cargo xtask context-digest --crate memfuse-db

# Output: Alle Tags + FILE-CONTEXT-Header + offene ANCHORs für dieses Crate
```

#### `cargo xtask audit-verify <AUDIT-ID>` (neu)

Prüft ob ein externer Audit-Fund noch zutrifft:

```bash
cargo xtask audit-verify AUDIT-2026-09-001 --file crates/memfuse-index/src/hnsw.rs --line 456
# Output: VALID ✓ | ALREADY_FIXED ✓ | FALSE_POSITIVE
```

### 2.3 Session-Context Digest Erweiterung

```bash
# just session-context (erweitert)
OFFENE KRITISCHE TAGS: (count + Liste)
OFFENE ANCHORS IN-PROGRESS: (count + Liste)
LETZTE 3 ADRs: (aus DECISIONS.md)
OPEN TAGS NACH CRATE: (Tabelle)
WORKING_STATE.md (letzte 20 Zeilen)
```

### 2.4 CI Gate 7 Härtung

```yaml
# context-gates.yml Gate 7 (erweitert)
- name: "Gate 7: TS UND SESSION Pflichtfelder auf allen neuen Tags"
  run: |
    # Prüfe TS-Feld
    MISSING_TS=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
      | grep -vE "TS:[0-9]{4}-[0-9]{2}-[0-9]{2}T" || true)
    
    # Prüfe SESSION-Feld (nur auf NEUEN Tags nach 2026-08-29)
    MISSING_SESSION=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
      | grep -E "TS:2026-0[89]|TS:202[7-9]" \
      | grep -v "SESSION:" || true)
    
    [ -n "$MISSING_TS" ] && echo "❌ TS-Feld fehlt:" && echo "$MISSING_TS" && exit 1
    [ -n "$MISSING_SESSION" ] && echo "❌ SESSION-Feld fehlt (neue Tags):" && echo "$MISSING_SESSION" && exit 1
    echo "✅ Alle Tags haben TS: und SESSION: Felder"
```

---

## TEIL 3 — GOOGLE JULES REFACTORING PROMPT

> **Verwendung**: Diesen Prompt vollständig in das Jules-Task-Feld eingeben.  
> **Branch**: `feature/context-engineering-v3`  
> **Erwartete Dauer**: 45–90 Minuten (Jules-Session)  
> **Human Review**: PR-Review durch Projektleitung erforderlich

---

```
═══════════════════════════════════════════════════════════════════════════
MEMFUSE — CONTEXT ENGINEERING REFACTORING v3
Autonomous Codebase Self-Documentation Task
═══════════════════════════════════════════════════════════════════════════

## ZIEL

Verbessere das Inline-Kontextsystem des MemFuse-Projekts so, dass zukünftige
Jules-Sitzungen mit maximaler Präzision und minimaler Fehlerrate arbeiten können.
Diese Aufgabe ist rein mechanisch-dokumentarisch: kein Produktionscode wird
geändert, nur Kommentare, Dokumentationsdateien und Tooling.

Arbeite die folgenden 6 Phasen in dieser Reihenfolge ab. Führe nach jeder
Phase den angegebenen Verifikationsbefehl aus, bevor du fortfährst.


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 0 — SESSION-IDENTITÄT & BASELINE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

```bash
# Session-Hash und Timestamp generieren (für alle Tags in dieser Session)
SESSION=$(date -u +%Y%m%d%H%M%S | sha256sum | head -c 8)
TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SESSION: $SESSION | TS: $TS"

# Baseline prüfen
cargo check --workspace --exclude memfuse-tauri 2>&1 | tail -3
cargo test --workspace --exclude memfuse-tauri 2>&1 | tail -5
```

Alle Tests müssen grün sein, bevor du weitermachst.


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 1 — BUG-FIX: chrono_or_today() korrigieren
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: In `xtask/src/main.rs` gibt `chrono_or_today()` immer den
statischen String "2026-08-27" zurück. Alle autogenerierten Dokumente
zeigen deshalb ein falsches Datum.

AUFGABE: Ersetze die Funktion durch eine echte Systemdatum-Abfrage.

Da die xtask-Crate bereits `walkdir` und `regex` als Dependencies hat,
prüfe zuerst ob `chrono` bereits in `xtask/Cargo.toml` vorhanden ist:

```bash
grep "chrono" xtask/Cargo.toml
```

WENN chrono bereits vorhanden:
Ersetze die Funktion so:

```rust
fn chrono_or_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}
```

WENN chrono NICHT vorhanden:
Nutze stattdessen den `date`-Systemaufruf über std::process::Command:

```rust
fn chrono_or_today() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}
```

NIEMALS: `chrono` als neue Dependency hinzufügen ohne `.jules/` zu prüfen.
Zuerst `grep "chrono" xtask/Cargo.toml` ausführen!

VERIFIKATION:
```bash
cargo xtask sync-docs
# Prüfe: WORKING_STATE.md darf NICHT "2026-08-27" als Stand-Datum enthalten
grep "2026-08-27" WORKING_STATE.md && echo "FEHLER: Datum immer noch hardcoded" || echo "OK: Datum korrekt"
```

ANCHOR für diese Phase:
```rust
// Füge diese Zeile in xtask/src/main.rs nach der korrigierten Funktion ein:
// ANCHOR[DEBT:XTASK-DATE-001] STATUS:DONE (ID: AGT-XTASK-$SESSION) (TS: $TS) (SESSION: $SESSION)
// AUFGABE: chrono_or_today() lieferte statischen String "2026-08-27" — behoben durch Systemaufruf
// GATE:    grep -v "2026-08-27" WORKING_STATE.md
```
(Ersetze $SESSION und $TS durch die Werte aus Phase 0)


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 2 — FEHLENDE AGENTS.md FÜR 2 CRATES
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: `crates/memfuse-agent/` und `crates/memfuse-router/` haben kein
crate-level AGENTS.md. Neue Sitzungen müssen Invarianten aus dem Code
rekonstruieren — fehleranfällig.

AUFGABE: Lese zuerst den tatsächlichen Code, dann schreibe die AGENTS.md.

### 2a: memfuse-agent/AGENTS.md

Lies zuerst:
```bash
cat crates/memfuse-agent/src/lib.rs | head -60
cat crates/memfuse-agent/src/engine.rs | head -80
cat crates/memfuse-agent/src/audit.rs | head -40
grep "pub fn\|pub struct\|pub trait" crates/memfuse-agent/src/*.rs | head -30
```

Erstelle `crates/memfuse-agent/AGENTS.md` mit diesem Schema:

```markdown
# memfuse-agent — Crate-Level Agent Rules

## Critical Invariants

### Checkpoint-Execute-Commit-Audit Loop
[Beschreibe den tatsächlichen State-Machine-Loop aus dem Code]

### [Weitere Invarianten die du aus dem Code erkennst]

## Layer Position
Layer 3. Darf importieren: memfuse-db (L2), memfuse-checkpoint (L1),
memfuse-graph (L1), memfuse-core (L0). Darf NICHT importieren: memfuse-tauri (L4).

## Nicht-offensichtliche Entscheidungen
[Aus Code-Kommentaren und Rustdoc extrahieren]
```

### 2b: memfuse-router/AGENTS.md

Lies zuerst:
```bash
cat crates/memfuse-router/src/lib.rs
cat crates/memfuse-router/src/router.rs | head -80
cat crates/memfuse-router/src/profile.rs | head -60
grep "pub fn\|pub struct\|pub enum" crates/memfuse-router/src/*.rs | head -30
```

Erstelle `crates/memfuse-router/AGENTS.md` analog.

INVARIANTE FÜR BEIDE: AGENTS.md darf maximal 60 Zeilen lang sein.
Konzentriere dich auf: Critical Invariants, Layer-Position, Nicht-offensichtliche Entscheidungen.

VERIFIKATION:
```bash
ls crates/memfuse-agent/AGENTS.md crates/memfuse-router/AGENTS.md && echo "OK: Beide vorhanden"
wc -l crates/memfuse-agent/AGENTS.md crates/memfuse-router/AGENTS.md
# Beide müssen <= 60 Zeilen haben
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 3 — FILE-CONTEXT HEADERS (Prioritätsliste)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: 25+ große Dateien (>50 LOC) fehlen FILE-CONTEXT-Header.
Ohne diese muss Jules bei jeder Bearbeitung den Zweck und die Invarianten
aus dem Code rückschließen.

AUFGABE: Füge FILE-CONTEXT-Header zu den folgenden 15 Prioritätsdateien hinzu.
Lies ZUERST die komplette Datei (zumindest head -100 und grep nach pub fn/struct),
dann schreibe den Header.

TAG-FORMAT (verpflichtend):
```rust
// FILE-CONTEXT
// STAND:       <TS> (SESSION: <SESSION>)
// ZWECK:       <Ein Satz — was diese Datei tut>
// INVARIANTEN: <Komma-separierte Must-Haves — aus tatsächlichem Code extrahiert>
// HOTSPOTS:    <Zeilenbereiche oder Funktionen die am häufigsten geändert werden>
// SIEHE AUCH:  <Relevante ADRs oder rules/*.md>
// AGENT-NOTIZ: <Optional — max. 1 Satz für den nächsten Agenten>
```

Der Header kommt NACH vorhandenem `//!`-Rustdoc (nicht davor), als erste
`//`-Kommentarblock nach dem Rustdoc-Block. Maximale Länge: 8 Zeilen.

PRIORITÄTSLISTE (in dieser Reihenfolge bearbeiten):

**P1 — Systemkritische Kernel-Dateien:**

1. `crates/memfuse-index/src/hnsw.rs`
   - Lies: head -50, grep "unsafe\|SAFETY\|invariant\|ANCHOR" -n
   - ZWECK: HNSW-Vektorindex (Insert/Search/Delete/Persist)
   - INVARIANTEN: Aus ADR-017 und vorhandenen ANCHOR-Tags ableiten
   - HOTSPOTS: Suchpfad (greedy_search), Einfügepfad (insert), ef_construction-Guard
   - SIEHE AUCH: rules/simd_safety.md, ADR-017, ADR-034

2. `crates/memfuse-store/src/sstable.rs`
   - Lies: head -60, grep "pub fn\|SAFETY\|fsync" -n | head -20
   - ZWECK: Persistente, immutable SSTable-Dateien (Sorted String Table)
   - HOTSPOTS: Iterator, Bloom-Filter-Lookup, Merge-Logik

3. `crates/memfuse-db/src/lib.rs`
   - Lies: grep "pub fn\|pub struct\|INVARIANT" | head -30
   - ZWECK: Orchestrator-Facade (Layer 2) — öffentliche API der Collection
   - INVARIANTEN: Aus dem INVARIANT-Kommentar in Zeile 1 ableiten + AGENTS.md
   - HOTSPOTS: hybrid_search(), insert(), relate()
   - SIEHE AUCH: crates/memfuse-db/AGENTS.md

4. `crates/memfuse-core/src/traits.rs`
   - Lies: head -60, grep "pub trait\|fn " | head -25
   - ZWECK: Zentrale Trait-Definitionen (VectorIndex, GraphIndex, TextIndex, etc.)
   - INVARIANTEN: Trait-Default-Pflichttest-Regel (AGENTS.md §4)
   - HOTSPOTS: VectorIndex::search_at, GraphIndex::traverse_at Default-Impls
   - SIEHE AUCH: docs/TYPE_REGISTRY.md, ADR-035

5. `crates/memfuse-store/src/compaction.rs`
   - Lies: head -60, grep "pub fn\|ANCHOR\|lock\|tokio" | head -20
   - ZWECK: STCS-Compaction-Engine (Size-Tiered Compaction Strategy)
   - INVARIANTEN: Compaction darf keine laufenden Reads blockieren
   - HOTSPOTS: compact_sstables(), merge_sorted_iters()

**P2 — FFI & Protokoll-Grenzen:**

6. `crates/memfuse-py/src/lib.rs`
   - ZWECK: PyO3 FFI-Grenzschicht — Rust-Fehler müssen in Python-Exceptions konvertiert werden
   - INVARIANTEN: Alle MemFuseError → PyErr-Konvertierung vollständig; kein Panic darf FFI-Grenze überschreiten

7. `crates/memfuse-mcp/src/lib.rs`
   - ZWECK: stdio JSON-RPC 2.0 MCP-Server (kein HTTP! ADR-010)
   - INVARIANTEN: Transport ist ausschließlich stdin/stdout — niemals TCP/axum
   - SIEHE AUCH: ADR-010, rules/async-io.md

8. `crates/memfuse-ollama/src/client.rs`
   - ZWECK: HTTP-Client für lokale Ollama-Instanz (Embedding + Chat)
   - HOTSPOTS: embed(), chat_completion() — Timeouts und Retry-Logik

**P3 — Kern-Datenstrukturen:**

9.  `crates/memfuse-core/src/error.rs`
    - ZWECK: Einzige Fehler-Enum (MemFuseError) — alle Crates propagieren hierher
    - INVARIANTEN: KEINE neue Error-Enum in anderen Crates anlegen; immer hier erweitern

10. `crates/memfuse-core/src/types/domain.rs`
    - ZWECK: Domain-Types (TxId, DocId, CollectionId) — Newtype-Wrapper für Typ-Sicherheit
    - INVARIANTEN: TxId ist u64-Newtype, NIEMALS direkt aus SystemTime erzeugen (AGENTS.md §4)

11. `crates/memfuse-core/src/tx_buffer.rs`
    - ZWECK: Transaktion-Staging-Buffer zwischen Writes und WAL-Commit
    - INVARIANTEN: Bounded capacity enforced (AGT-CORE-001 behoben); kein unbounded growth

12. `crates/memfuse-checkpoint/src/lib.rs`
    - ZWECK: RAII CheckpointGuard + persistente Snapshot-Verwaltung
    - INVARIANTEN: CheckpointGuard darf NICHT mit PersistentCheckpointStore verwechselt werden

**P4 — Algorithmen:**

13. `crates/memfuse-text/src/inverted.rs`
    - ZWECK: BM25-Invertierter Index mit Tombstone-Update-Semantik
    - HOTSPOTS: insert(), delete() (Tombstone-Logik), query()

14. `crates/memfuse-text/src/morphology.rs`
    - ZWECK: Morphologie-Analyse (Stemming, Lemmatisierung für BM25)
    - HOTSPOTS: analyze() — Deutsch + Englisch Support

15. `crates/memfuse-graph/src/ppr.rs`
    - ZWECK: Personalized PageRank für GraphRAG Community Detection
    - HOTSPOTS: run_ppr(), power_iteration()
    - SIEHE AUCH: ADR-031

VERIFIKATION nach Phase 3:
```bash
# Prüfe dass alle 15 Dateien einen FILE-CONTEXT-Header haben
FILES=(
  "crates/memfuse-index/src/hnsw.rs"
  "crates/memfuse-store/src/sstable.rs"
  "crates/memfuse-db/src/lib.rs"
  "crates/memfuse-core/src/traits.rs"
  "crates/memfuse-store/src/compaction.rs"
  "crates/memfuse-py/src/lib.rs"
  "crates/memfuse-mcp/src/lib.rs"
  "crates/memfuse-ollama/src/client.rs"
  "crates/memfuse-core/src/error.rs"
  "crates/memfuse-core/src/types/domain.rs"
  "crates/memfuse-core/src/tx_buffer.rs"
  "crates/memfuse-checkpoint/src/lib.rs"
  "crates/memfuse-text/src/inverted.rs"
  "crates/memfuse-text/src/morphology.rs"
  "crates/memfuse-graph/src/ppr.rs"
)
MISSING=0
for f in "${FILES[@]}"; do
  grep -q "FILE-CONTEXT" "$f" || { echo "FEHLT: $f"; MISSING=$((MISSING+1)); }
done
echo "Fehlende FILE-CONTEXT: $MISSING (Ziel: 0)"
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 4 — DUPLIKATE ANCHORS BEREINIGEN
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: Mehrere Dateien haben identische ANCHOR-Zeilen auf aufeinander-
folgenden Zeilen. Das verursacht falschen Zählstand im Tag-Scan.

AUFGABE: Identifiziere und entferne die Duplikate.

Identifikation:
```bash
# Finde alle Dateien mit aufeinanderfolgenden identischen ANCHOR-Zeilen
grep -rn "ANCHOR\[" crates/ --include="*.rs" | \
  awk -F: 'prev==$1 ":" $2+1 && prevtext==$3 {print $0} {prev=$1; prevtxt=$3}' || true

# Einfachere Alternative:
for f in $(grep -rl "ANCHOR\[" crates/ --include="*.rs"); do
  awk '/ANCHOR\[/{if($0==last){print FILENAME ":" NR ": DUPLIKAT: " $0} last=$0}' "$f"
done
```

Entferne nur echte Duplikate (identischer Text auf aufeinanderfolgenden Zeilen).
Behalte verschiedene ANCHORs auf verschiedenen Zeilen.

VERIFIKATION:
```bash
# Nach Bereinigung: keine aufeinanderfolgenden identischen ANCHOR-Zeilen
for f in $(grep -rl "ANCHOR\[" crates/ --include="*.rs"); do
  awk '/ANCHOR\[/{if($0==last){print FILENAME ":" NR ": DUPLIKAT NOCH VORHANDEN: " $0} last=$0}' "$f"
done
echo "Duplikat-Check abgeschlossen"
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 5 — CI GATE 7 HÄRTUNG (SESSION-Feld)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: `.github/workflows/context-gates.yml` Gate 7 prüft nur das TS:-
Feld, aber nicht das SESSION:-Feld. Gemäß `rules/tag_taxonomy.md` sind
beide für alle neuen Tags (nach 2026-08-29) verpflichtend.

AUFGABE: Erweitere Gate 7 in `.github/workflows/context-gates.yml`.

LIES ZUERST die aktuelle Gate-7-Implementierung:
```bash
grep -A 15 "Gate 7" .github/workflows/context-gates.yml
```

Ersetze den Gate-7-Step durch:
```yaml
      - name: "Gate 7: TS UND SESSION Pflichtfelder auf allen neuen Tags"
        run: |
          # Prüfe TS-Feld auf allen Tags
          MISSING_TS=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
            | grep -vE "TS:[0-9]{4}-[0-9]{2}-[0-9]{2}T" || true)
          if [ -n "$MISSING_TS" ]; then
            echo "❌ Tags ohne gültigen TS:-Zeitstempel:"
            echo "$MISSING_TS"
            exit 1
          fi
          
          # Prüfe SESSION-Feld auf NEUEN Tags (Datum >= 2026-08-29)
          MISSING_SESSION=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
            | grep -E "TS:2026-0[89]-[0-9]{2}T|TS:202[7-9]-" \
            | grep -v "SESSION:" || true)
          if [ -n "$MISSING_SESSION" ]; then
            echo "❌ Neue Tags (>= 2026-08-29) ohne SESSION:-Feld:"
            echo "$MISSING_SESSION"
            echo "Füge SESSION: <8-hex> zu diesen Tags hinzu."
            exit 1
          fi
          echo "✅ Alle Tags haben TS: und neue Tags haben SESSION: Felder"
```

VERIFIKATION:
```bash
# Simuliere den neuen Gate-7-Check lokal
MISSING_SESSION=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
  | grep -E "TS:2026-0[89]-[0-9]{2}T|TS:202[7-9]-" \
  | grep -v "SESSION:" || true)
[ -n "$MISSING_SESSION" ] && echo "FEHLER: $MISSING_SESSION" || echo "OK: Alle neuen Tags haben SESSION-Feld"
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
PHASE 6 — XTASK ERWEITERUNG: context-tags SUBCOMMAND
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

PROBLEM: Das Cheat-Sheet dokumentiert `cargo xtask context-tags --crate X
--severity CRITICAL` mit NDJSON-Output. Dieser Subcommand existiert nicht.
Jules muss auf fehleranfällige grep-Aufrufe ausweichen.

AUFGABE: Implementiere den `context-tags` Subcommand in `xtask/src/main.rs`.

LIES ZUERST die bestehende xtask-Architektur:
```bash
grep "fn main\|fn run_\|match subcommand" xtask/src/main.rs | head -20
grep "pub struct TagItem\|pub fn scan_tags" xtask/src/main.rs | head -5
```

Die Funktion `scan_tags()` und `TagItem`-Struct existieren bereits.
Implementiere NUR die CLI-Verarbeitung und NDJSON-Serialisierung.

KEINE neue externe Dependency verwenden. Nutze `serde_json` wenn vorhanden,
sonst manuelle JSON-String-Konstruktion.

```bash
# Prüfe ob serde_json bereits in xtask verfügbar ist:
grep "serde_json" xtask/Cargo.toml
```

WENN serde_json vorhanden, nutze es für JSON-Serialisierung.
WENN nicht vorhanden, baue JSON-Strings manuell (einfacher als Dep hinzufügen).

IMPLEMENTIERUNG — füge in `fn main()` den neuen Match-Arm hinzu:

```rust
"context-tags" => {
    // Parse Flags: --crate, --severity, --status, --type
    let filter_crate = args.iter()
        .position(|a| a == "--crate")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    
    let filter_severity = args.iter()
        .position(|a| a == "--severity")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_uppercase());
    
    let filter_status = args.iter()
        .position(|a| a == "--status")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_uppercase());
    
    let filter_type = args.iter()
        .position(|a| a == "--type")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_uppercase());
    
    let tags = scan_tags("crates");
    
    for tag in &tags {
        // Crate-Filter: Prüfe ob Dateipfad den Crate-Namen enthält
        if let Some(crate_name) = filter_crate {
            if !tag.file_path.contains(crate_name) { continue; }
        }
        // Severity-Filter
        if let Some(ref sev) = filter_severity {
            if tag.severity.as_deref().map(|s| s.to_uppercase()).as_deref() != Some(sev.as_str()) { continue; }
        }
        // Status-Filter
        if let Some(ref sta) = filter_status {
            let tag_status = tag.status.as_deref().map(|s| s.to_uppercase()).unwrap_or_default();
            if &tag_status != sta { continue; }
        }
        // Type-Filter
        if let Some(ref typ) = filter_type {
            if &tag.tag_type.to_uppercase() != typ { continue; }
        }
        
        // NDJSON-Ausgabe (eine JSON-Zeile pro Tag)
        println!(
            "{{\"file\":{:?},\"line\":{},\"type\":{:?},\"cat\":{:?},\"sev\":{:?},\"id\":{:?},\"ts\":{:?},\"session\":{:?},\"status\":{:?},\"resolved\":{},\"desc\":{:?}}}",
            tag.file_path,
            tag.line_num,
            tag.tag_type,
            tag.category.as_deref().unwrap_or(""),
            tag.severity.as_deref().unwrap_or(""),
            tag.id.as_deref().unwrap_or(""),
            tag.timestamp,
            tag.session.as_deref().unwrap_or(""),
            tag.status.as_deref().unwrap_or(""),
            tag.is_resolved,
            tag.description,
        );
    }
}
```

Füge auch den Hilfstext im `other`-Branch hinzu:
```rust
eprintln!("Available commands: sync-docs [--check], check-consistency, \
           check-review-coverage, run-community-detection, \
           context-tags [--crate NAME] [--severity LEVEL] [--status STATUS] [--type TYPE]");
```

Füge außerdem einen Eintrag im `justfile` hinzu:
```makefile
# Zeigt alle Context-Tags als NDJSON (filterbar nach Crate, Severity, Status)
context-tags *ARGS:
    cargo xtask context-tags {{ARGS}}
```

VERIFIKATION:
```bash
cargo build -p xtask 2>&1 | tail -5
cargo xtask context-tags --severity CRITICAL --status OPEN | head -5
cargo xtask context-tags --crate memfuse-db | head -10
echo "context-tags Subcommand: OK"
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
FINALE VALIDIERUNG
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Führe ALLE der folgenden Checks durch. Alle müssen grün sein:

```bash
# 1. Alle Tests bestehen noch
cargo test --workspace --exclude memfuse-tauri 2>&1 | tail -5

# 2. Code ist formatiert
cargo fmt --all

# 3. Keine neuen Clippy-Fehler
cargo clippy --workspace --exclude memfuse-tauri -- -D warnings 2>&1 | tail -10

# 4. Datum in autogenerierten Docs ist aktuell (nicht 2026-08-27)
cargo xtask sync-docs
grep -v "2026-08-27" WORKING_STATE.md > /dev/null && echo "✅ Datum korrekt" || echo "❌ Datum hardcoded"

# 5. Beide neuen AGENTS.md vorhanden
ls crates/memfuse-agent/AGENTS.md crates/memfuse-router/AGENTS.md && echo "✅ AGENTS.md vorhanden"

# 6. Mindestens 15 FILE-CONTEXT Header vorhanden
COUNT=$(grep -rl "FILE-CONTEXT" crates/ --include="*.rs" | wc -l)
echo "FILE-CONTEXT Coverage: $COUNT Dateien (Ziel: >= 20)"

# 7. context-tags Subcommand funktioniert
cargo xtask context-tags --type AI-TAG --status OPEN | wc -l
echo "✅ context-tags liefert Output"

# 8. Keine duplizierten aufeinanderfolgenden ANCHOR-Zeilen
DUPS=$(for f in $(grep -rl "ANCHOR\[" crates/ --include="*.rs"); do
  awk '/ANCHOR\[/{if($0==last){print FILENAME ":" NR} last=$0}' "$f"
done)
[ -z "$DUPS" ] && echo "✅ Keine Duplikate" || echo "❌ Duplikate: $DUPS"

# 9. Sync-Docs Check bestehen
cargo xtask sync-docs --check && echo "✅ Docs synchron"
```


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
COMMIT & PULL REQUEST
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

```bash
git add -A
git commit -m "Context Engineering v3: FILE-CONTEXT coverage, AGENTS.md, xtask fix

- Fix: chrono_or_today() liefert jetzt aktuelles Systemdatum
- Feat: AGENTS.md für memfuse-agent und memfuse-router
- Feat: FILE-CONTEXT-Header für 15 Prioritätsdateien
- Fix: Duplizierte ANCHOR-Einträge entfernt
- Feat: CI Gate 7 prüft jetzt auch SESSION-Feld
- Feat: cargo xtask context-tags mit NDJSON-Output

Alle Tests grün. cargo xtask sync-docs --check bestanden."

git push origin feature/context-engineering-v3
```

PR-Titel: `[Context Engineering v3] FILE-CONTEXT Coverage + xtask Fix + CI Härtung`

PR-Beschreibung muss enthalten:
- Anzahl der hinzugefügten FILE-CONTEXT-Header
- Link zu den zwei neuen AGENTS.md-Dateien
- Verifikationsoutput der finalen Checks


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
EINSCHRÄNKUNGEN (NIEMALS dagegen verstoßen)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

- KEINE Änderungen an Produktionscode (nur Kommentare, Docs, Tooling)
- KEINE neuen externen Dependencies ohne explizite Prüfung (Cargo.toml lesen!)
- KEINE Änderungen an public API-Signaturen
- KEINE unsafe-Code-Änderungen
- KEINE Änderungen an der Tag-Taxonomie (rules/tag_taxonomy.md bleibt unberührt)
- ALLE vorhandenen Tests müssen WEITERHIN grün bleiben
- FILE-CONTEXT-Header dürfen NIEMALS vorhandenes Rustdoc (//!) ersetzen
- AGENTS.md darf maximal 60 Zeilen haben (pro Crate)
- ALLE neuen Tags in dieser Session tragen SESSION: <deinen SESSION-Hash>


━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
FERTIG-WENN (Done-When Kriterien)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ `cargo test --workspace --exclude memfuse-tauri` → alle grün
✅ `grep "2026-08-27" WORKING_STATE.md` → kein Match (Datum korrekt)
✅ `ls crates/memfuse-agent/AGENTS.md crates/memfuse-router/AGENTS.md` → beide vorhanden
✅ `grep -rl "FILE-CONTEXT" crates/ --include="*.rs" | wc -l` → mindestens 20
✅ `cargo xtask context-tags --severity CRITICAL --status OPEN` → Befehl existiert und gibt Output
✅ `cargo xtask sync-docs --check` → grün (keine Dokumentationsdrift)
✅ Pull Request auf `feature/context-engineering-v3` erstellt

═══════════════════════════════════════════════════════════════════════════
ENDE DES PROMPTS
═══════════════════════════════════════════════════════════════════════════
```

---

## TEIL 4 — IMPLEMENTIERUNGSROADMAP (Post-v3)

Nach erfolgreichem Merge von v3 sind die folgenden Maßnahmen für spätere Jules-Sitzungen vorbereitet:

### v3.1 — Session-Context Digest Upgrade
`just session-context` erweitern um: offene Tags nach Crate (Tabelle), letzte 3 ADRs, WORKING_STATE.md-Tail (20 Zeilen). Geschätzte Dauer: 30 Minuten.

### v3.2 — `cargo xtask audit-verify` Implementierung
Automatische Verifikation externer Audit-Findings gegen aktuellen Quellcode. Eliminiert den manuellen Schritt aus `.jules/AUDIT_INTAKE_PROTOCOL.md`. Geschätzte Dauer: 60 Minuten.

### v3.3 — FILE-CONTEXT Restabdeckung
Die verbleibenden 10+ großen Dateien (z.B. `memfuse-store/src/memtable.rs`, `memfuse-db/src/collection/search.rs`) bekommen FILE-CONTEXT-Header. Kann mit einem ähnlichen Jules-Prompt wie v3 beauftragt werden.

### v3.4 — GitHub Actions: Täglicher Kontext-Qualitäts-Scan
```yaml
# .github/workflows/context-quality.yml
on:
  schedule:
    - cron: '0 6 * * *'
jobs:
  context-quality:
    steps:
      - name: Prüfe FILE-CONTEXT Coverage (Ziel: 80% der Dateien >50 LOC)
        run: |
          TOTAL=$(find crates/ -name "*.rs" -not -path "*/tests/*" \
            | xargs wc -l 2>/dev/null | awk '$1 > 50' | wc -l)
          COVERED=$(grep -rl "FILE-CONTEXT" crates/ --include="*.rs" | wc -l)
          echo "Coverage: $COVERED/$TOTAL Dateien"
```

---

## APPENDIX A — FORSCHUNGSERGEBNISSE ZU JULES BEST PRACTICES

Aus der Analyse von Repositories mit Google Jules-Beteiligung (`google-labs-code/jules-awesome-list`, `google-labs-code/jules-action`, `google-labs-code/design.md`) sowie Praxis-Guides (MachineLearningMastery, KDnuggets, DataCamp):

| Erkenntnis | Quelle | Umgesetzt in v3 |
|---|---|---|
| AGENTS.md >150 Zeilen: +20–23% Inference-Kosten, kein Performancegewinn | betterclaw.io Research 2.500 Repos | ✅ Crate-AGENTS.md auf ≤60 Zeilen limitiert |
| Jules ist „planning-first": Prompt mit klarer Phasenstruktur → bessere Pläne | machinelearningmastery.com | ✅ 6-Phasen-Struktur mit Verifikationsgates |
| Vier-Element-Prompt: ZIEL / KONTEXT / EINSCHRÄNKUNGEN / FERTIG-WENN | ai-boost/awesome-prompts | ✅ Alle vier Elemente vorhanden |
| Jules liest README.md, CONTRIBUTING.md, AGENTS.md at session start | kdnuggets.com | ✅ @-Import-Kette: JULES.md → AGENTS.md → WORKING_STATE.md |
| Kleine, explizite Tasks mit verifizierbaren Done-Criteria | machineLearningmastery.com | ✅ Shell-Befehle als Verifikation |
| Inline-Dokumentation > externe Specs für LLM-Autonomie | google-labs-code/design.md Prinzip | ✅ FILE-CONTEXT direkt im Code |
| NDJSON/maschinenlesbarer Output für LLM-Tool-Calls | Jules SDK Beispiele | ✅ context-tags Subcommand |

---

## APPENDIX B — KOMMENTAR-SYSTEM REFERENZKARTE

```
┌─────────────────────────────────────────────────────────────────────┐
│                    MEMFUSE TAG SYSTEM v3.0                          │
├──────────────┬──────────────────────────────────────────────────────┤
│  AI-TAG      │ Aktuelle Probleme/Risiken im Code                    │
│  Format:     │ // AI-TAG[KAT][SEV] Beschreibung (ID: AGT-X-<hash>) │
│              │ //   (TS: YYYY-MM-DDTHH:MM:SSZ) (SESSION: <8-hex>)  │
│              │ // BEFUND:     Was ist falsch                        │
│              │ // RISIKO:     Warum es wichtig ist                  │
│              │ // EMPFEHLUNG: Wie zu beheben                        │
│  Severities: │ BLOCKER > CRITICAL > MAJOR > MINOR                  │
│  CI:         │ BLOCKER/CRITICAL → Gate 1 bricht ab                 │
├──────────────┼──────────────────────────────────────────────────────┤
│  ANCHOR      │ Geplante/laufende Arbeit                             │
│  Format:     │ // ANCHOR[TYP:ID] STATUS:X (TS: ...) (SESSION: ...) │
│              │ // AUFGABE: Was zu implementieren ist                │
│              │ // GATE:    cargo test -p <crate> --test <name>      │
│  Status:     │ OPEN → IN-PROGRESS AGENT:N → DONE / BLOCKED         │
│  CI (Gate8): │ DONE erfordert 2–3 unabh. REVIEW-PASS               │
├──────────────┼──────────────────────────────────────────────────────┤
│  REVIEW-PASS │ Unabhängige Multi-Session-Reviews                    │
│  Format:     │ // REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL    │
│              │ //   (ID: AGT-X-<hash>) (TS: ...) (SESSION: ...)    │
│              │ // PRÜFER-KONTEXT: FRESH                             │
│              │ // BEFUND: Was geprüft/bestätigt                     │
├──────────────┼──────────────────────────────────────────────────────┤
│  FILE-CTX    │ Datei-Kontext für Agenten (v3: +HOTSPOTS Feld)      │
│  Format:     │ // FILE-CONTEXT                                      │
│              │ // STAND:       <TS> (SESSION: <hash>)               │
│              │ // ZWECK:       Ein Satz — was diese Datei tut       │
│              │ // INVARIANTEN: Komma-separierte Must-Haves          │
│              │ // HOTSPOTS:    Zeilenbereiche/Funktionen             │
│              │ // SIEHE AUCH:  ADRs/rules/*.md                      │
│              │ // AGENT-NOTIZ: Optional, max. 1 Satz                │
│  Limit:      │ Max. 8 Zeilen. Kein Ersatz für Rustdoc!             │
└──────────────┴──────────────────────────────────────────────────────┘

QUERIES (nach v3 verfügbar):
  cargo xtask context-tags --severity CRITICAL --status OPEN
  cargo xtask context-tags --crate memfuse-db
  cargo xtask context-tags --type ANCHOR --status IN-PROGRESS
  just session-context  (erweiterter Digest)
  cargo xtask sync-docs (regeneriert WORKING_STATE.md etc.)
```

---

*Erstellt durch Context-Engineering-Analyse von claude-sonnet-4-6 | Version 1.0 | 2026-08-29*
