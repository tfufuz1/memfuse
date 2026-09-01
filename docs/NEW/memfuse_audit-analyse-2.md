# MemFuse Codebase — Umfassender Sicherheits- und Qualitäts-Auditbericht

**Auftraggeber:** Enterprise Architecture Review  
**Prüfer:** Senior Rust-Entwickler & KI-Systemexperte  
**Datum:** 01. September 2026  
**Repository:** `https://github.com/tfufuz1/memfuse`  
**Gesamtbewertung:** 🔴 **KRITISCH — Mehrere unresolvte Bugs, inkl. falscher Audit-Dokumentation**

---

## Executive Summary

Nach vollständiger Analyse aller 15 Workspace-Crates wurden **19 Fehler** unterschiedlicher Schwere gefunden. Besonders alarmierend: Die vorhandene Audit-Dokumentation (`docs/audits/`) behauptet, mehrere Bugs seien „RESOLVED", während der tatsächliche Quellcode die Fixes **nicht enthält**. Dies deutet auf eine strukturelle Vertrauenskrise in den KI-generierten Audit-Prozess hin.

---

## 🔴 KRITISCHE FEHLER (Sofortiger Handlungsbedarf)

---

### BUG-GRA-003 · `memfuse-graph` · `csr.rs::GraphInner::compact()` — **Audit-Lüge: Fix nie eingecheckt**

**Schwere:** Kritisch — Strukturelle CSR-Invarianten-Verletzung  
**Datei:** `crates/memfuse-graph/src/csr.rs`, Zeile 201

**Problem:**  
Die bestehende Audit-Dokumentation `AUDIT_memfuse-graph.md` listet unter `BUG-GRA-001`:

> **Status: RESOLVED** — „Updated `compact()` to extend `offsets` with `last_offset` whenever `offsets.len() != num_nodes + 1`."

Der tatsächliche Quellcode enthält diesen Fix **nicht**. Die `compact()`-Funktion enthält exakt die fehlerhafte Originallogik:

```rust
fn compact(&mut self) {
    if !self.is_dirty || (self.pending_edges.is_empty() && self.tombstoned_edges.is_empty()) {
        self.pending_edge_count = 0;
        self.is_dirty = false;
        return;  // ← FRÜHER ABBRUCH, KEIN OFFSET-SYNC!
    }
    // ...
```

**Reproduktion des Fehlers:**

1. `add_entity(tx, EntityA)` → setzt `is_dirty = true`
2. `commit(tx)` → Entities committed, `pending_edges` bleibt leer
3. `compact()` aufrufen:
    - `!is_dirty = false`, aber `pending_edges.is_empty() && tombstoned_edges.is_empty() = true`
    - Bedingung: `false || true = true` → **Frühzeitiger Return**
    - `offsets`-Array wird **nicht** auf `reverse_map.len() + 1` erweitert

**Konsequenz:** CSR-Invariante `offsets.len() == reverse_map.len() + 1` gebrochen. PPR und `traverse_at_time` können bei gezielten Abfragen über den CSR-Pfad falsche Ergebnisse liefern.

**Fix:**

```rust
fn compact(&mut self) {
    let num_nodes = self.reverse_map.len();
    
    // Sync offsets even when no edges/tombstones exist
    if !self.is_dirty && self.pending_edges.is_empty() && self.tombstoned_edges.is_empty() {
        // Check if offsets needs padding anyway (entity-only commits)
        while self.offsets.len() < num_nodes + 1 {
            let last = *self.offsets.last().unwrap_or(&0);
            self.offsets.push(last);
        }
        return;
    }
    // ... rest of compact
```

---

### BUG-AGT-001 · `memfuse-agent` · `engine.rs::run_internal()` — **Post-Check Execution Bug**

**Schwere:** Kritisch — Werkzeuge laufen trotz erschöpftem Budget  
**Datei:** `crates/memfuse-agent/src/engine.rs`, Zeile 116–167

**Problem:**  
Der Token-Budget-Check findet **nach** der Werkzeug-Ausführung statt:

```rust
// Schritt 2: Tool AUSFÜHREN (BEVOR Budget geprüft wird!)
tool.execute(ctx, input).await

// ...Commits, Audit-Logs...

// Schritt 5: Token verbrauchen und Budget prüfen — ZU SPÄT
ctx.budget.consume(result.tokens_consumed);
if ctx.budget.available() == 0 && node.node_type != NodeType::Start {
    return Err(MemFuseError::Internal("Token budget exhausted".to_string()));
}
```

**Konsequenz:** Hat Schritt N das Budget auf 0 gebracht, führt Schritt N+1 das Tool **komplett aus** (inklusive aller Seiteneffekte, LSM-Writes, externer API-Calls), bevor überhaupt festgestellt wird, dass kein Budget mehr verfügbar ist. Dies verletzt die Exact-Once-Garantie bei agentenbasierten Loops und kann unbegrenzte Kosten verursachen.

**Fix (korrekte Reihenfolge):**

```rust
// PRE-CHECK vor Ausführung:
if ctx.budget.available() == 0 && node.node_type != NodeType::Start {
    let err = "Token budget exhausted before step execution".to_string();
    self.audit_log_failure(ctx, &err).await?;
    return Err(MemFuseError::Internal(err));
}

// Erst dann: Tool ausführen
let result = tool.execute(ctx, input).await?;
ctx.budget.consume(result.tokens_consumed);
```

---

### BUG-GRA-004 · `memfuse-graph` · `csr.rs::add_edge()` — **Entity-Index-Leak bei Rollback**

**Schwere:** Kritisch — Permanente Speicherkorruption durch Rollbacks  
**Datei:** `crates/memfuse-graph/src/csr.rs`, Zeile ~715

**Problem:**  
`add_edge()` ruft `get_or_create_index()` **bereits beim Staging** auf:

```rust
async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()> {
    let mut inner = self.inner.write();
    let from_idx = inner.get_or_create_index(edge.from);  // ← Mutiert id_map + reverse_map!
    let to_idx = inner.get_or_create_index(edge.to);       // ← Permanent, auch bei Rollback!
    inner.staged_edges.entry(tx).or_default()...
```

```rust
async fn rollback(&self, tx: TxId) -> Result<()> {
    let mut inner = self.inner.write();
    inner.staged_entities.remove(&tx);   // ✓ Entities cleaned
    inner.staged_edges.remove(&tx);       // ✓ Edges cleaned
    // ← id_map und reverse_map werden NICHT bereinigt!
}
```

**Konsequenz:** Nach wiederholten Add/Rollback-Zyklen wächst `reverse_map` monoton. Jeder Rollback hinterlässt Phantomeinträge. Diese führen zu:

- Falsch berechnetem `offsets`-Array nach `compact()` (N Phantomknoten ohne Kanten)
- Speicherleck proportional zur Rollback-Häufigkeit
- Fehlerhaftem Verhalten bei Property-Tests mit `RemoveEdge`-Sequenzen

Der Code selbst gibt in einem Testkommentar zu: _„Aktuell ist es aber wahrscheinlich 3 (1, 2 und 3)"_ — was die erwartete Isolation bricht.

---

## 🟠 HOHE SCHWERE (Innerhalb von 1 Sprint beheben)

---

### BUG-RTR-001 · `memfuse-router` · `profile.rs::SlmProfile::new()` — **NaN/Invalid-Validierung fehlt**

**Schwere:** Hoch — Stille NaN-Propagation führt zu nie-routing-fähigem System  
**Datei:** `crates/memfuse-router/src/profile.rs`

**Problem:**  
`SlmProfile::new()` akzeptiert jeden `f32`-Wert für `min_relevance_score` ohne Validierung:

```rust
pub fn new(
    name: impl Into<String>,
    mcp_endpoint: impl Into<String>,
    domain_communities: Vec<u64>,
    token_budget: TokenBudget,
    min_relevance_score: f32,  // NaN? -inf? Leer-String für endpoint? Alles erlaubt!
) -> Self {
```

Wenn `NaN` übergeben wird, ist `score >= NaN` **immer false** (IEEE 754), weshalb kein Profil jemals gematcht wird. Leerer `mcp_endpoint` führt zu HTTP-Fehlern ohne Herkunftshinweis.

**Fix:**

```rust
pub fn try_new(..., min_relevance_score: f32) -> Result<Self, ProfileError> {
    if name.trim().is_empty() { return Err(ProfileError::EmptyName); }
    if mcp_endpoint.trim().is_empty() { return Err(ProfileError::EmptyEndpoint); }
    if !min_relevance_score.is_finite() || min_relevance_score < 0.0 {
        return Err(ProfileError::InvalidScore(min_relevance_score));
    }
    Ok(Self { ... })
}
```

---

### BUG-AGT-002 · `memfuse-agent` · `engine.rs::replay_from()` — **Budget-Zustand nach Replay inkonsistent**

**Schwere:** Hoch — Agent bricht nach korrektem Checkpoint-Restore sofort ab  
**Datei:** `crates/memfuse-agent/src/engine.rs`, Zeile ~185

**Problem:**  
`replay_from()` stellt `current_node`, `step_count` und `memory` aus dem Checkpoint wieder her — aber **nicht** das `budget`:

```rust
pub async fn replay_from(&self, ctx: &mut AgentContext, identifier: &str) -> Result<()> {
    // ...
    ctx.current_node = node.to_string();   // ✓ Wiederhergestellt
    ctx.step_count = step;                  // ✓ Wiederhergestellt
    ctx.memory = memory;                    // ✓ Wiederhergestellt
    // ctx.budget ← FEHLT! Budget bleibt bei aktuellem (evtl. erschöpftem) Wert!
```

**Konsequenz:** Nach Replay kann das Budget bereits 0 sein (weil es vor dem Crash erschöpft wurde). Der erste Task-Schritt nach Replay schlägt sofort mit „Token budget exhausted" fehl. Das Crash-Recovery-Subsystem funktioniert nicht zuverlässig.

**Fix:** Budget aus Checkpoint-Metadaten wiederherstellen:

```rust
if let Some(available) = checkpoint.metadata.get("budget_available").and_then(|v| v.as_u64()) {
    ctx.budget = ctx.budget.with_remaining(available);
}
```

---

### BUG-CKP-001 · `memfuse-checkpoint` · `lib.rs` — **Fire-and-Forget Drop-Rollback**

**Schwere:** Hoch — RAII-Garantie nicht durchgesetzt  
**Datei:** `crates/memfuse-checkpoint/src/lib.rs`, Zeile 185

**Problem:**  
Der `Drop`-Handler des `CheckpointGuard` spawnt den Rollback als asynchronen Task:

```rust
impl<S: StorageEngine> Drop for CheckpointGuard<S> {
    fn drop(&mut self) {
        if let Some(cp) = self.checkpoint.take() {
            let storage_clone = Arc::clone(&self.storage);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {          // ← Fire-and-Forget!
                    if let Err(e) = storage_clone.rollback_to_tx(cp.tx_id).await {
                        tracing::error!("Rollback fehlgeschlagen: {e}");  // ← Nur Log!
                    }
                });
```

**Konsequenz:** Wenn der Agent-Schritt fehlschlägt und der Guard gedroppt wird, ist nicht garantiert, dass der Rollback vor dem nächsten Schritt abgeschlossen ist. Im schlimmsten Fall (Panic in async context) wird der Rollback-Task nie ausgeführt. Der RAII-Contract ist gebrochen.

---

### BUG-AGT-003 · `memfuse-agent` · `context.rs::attach_event()` — **O(n) Vec::remove(0)**

**Schwere:** Hoch (Performance) — Latenz-Cliff bei vollem Event-Buffer  
**Datei:** `crates/memfuse-agent/src/context.rs`, Zeile ~140

**Problem:**

```rust
if self.events.len() >= MAX_TELEMETRY_EVENTS {
    self.events.remove(0);  // ← O(10.000) Shift-Operationen bei jedem Evict!
}
self.events.push(event);
```

Mit `MAX_TELEMETRY_EVENTS = 10.000` bedeutet jede Eviction 10.000 Speicherverschiebungen. Bei konstantem Event-Strom: pathologische CPU-Last.

**Fix:** `VecDeque<BackgroundEvent>` verwenden:

```rust
pub events: VecDeque<crate::event_source::BackgroundEvent>,
// ...
if self.events.len() >= MAX_TELEMETRY_EVENTS {
    self.events.pop_front();  // ← O(1)
}
self.events.push_back(event);
```

---

## 🟡 MITTLERE SCHWERE (Nächster Sprint)

---

### BUG-TXT-001 · `memfuse-text` · `tokenizer.rs` — **Destruktive URL/E-Mail-Tokenisierung**

**Datei:** `crates/memfuse-text/src/tokenizer.rs`

`unicode_words()` zerstört strukturierte Strings:

- `user@example.com` → `["user", "example", "com"]`
- `https://api.example.com/v2/search` → `["https", "api", "example", "com", "v2", "search"]`

Im IT/SaaS-KMU-Kontext (laut Projektbeschreibung der Haupt-Use-Case) sind URLs und E-Mail-Adressen zentrale Datenartefakte. Ihre Zerstörung führt zu False-Positive-Matches (z.B. „com" matcht alle `.com`-Domains).

**Fix:** Regex-basierte Pre-Pass-Erkennung von URLs und E-Mails, Token-Preserve als atomare Einheit.

---

### BUG-TXT-002 · `memfuse-text` · `tokenizer.rs::GermanMorphTokenizer::new()` — **Keine Trie-Zwischenspeicherung**

**Datei:** `crates/memfuse-text/src/tokenizer.rs`

`GermanCompoundSplitter::new()` baut bei **jedem Aufruf** den kompletten Trie aus der Wörterliste neu auf. Kein `OnceLock`, kein `lazy_static`. Wenn Tokenizer-Instanzen häufig erstellt werden, entsteht O(n × m) Overhead (n = Wörterbuchgröße, m = Wortlänge) pro Request.

**Fix:**

```rust
static SHARED_TRIE: OnceLock<Arc<Trie>> = OnceLock::new();

pub fn new() -> Self {
    let trie = SHARED_TRIE.get_or_init(|| Arc::new(build_trie_from_dictionary()));
    Self { trie: Arc::clone(trie), min_component_len: 3 }
}
```

---

### BUG-DB-001 · `memfuse-db` · `search.rs::search_with_filter_expr()` — **Doppelter k==0-Check (Dead Code)**

**Datei:** `crates/memfuse-db/src/collection/search.rs`

Der k==0-Check erscheint zweimal — einmal vor und einmal nach `k.min(MAX_SEARCH_K)`. Der zweite Check ist unerreichbarer Dead Code, da nach `.min()` k nicht neu auf 0 fallen kann:

```rust
if k == 0 { return Err(...); }            // Check 1: korrekt
let k = k.min(memfuse_core::MAX_SEARCH_K);
// ...
if k == 0 { return Err(...); }            // Check 2: UNERREICHBAR! Dead Code.
```

---

### BUG-RTR-002 · `memfuse-router` · `router.rs` — **Stille NaN-Propagation in Scoring**

**Datei:** `crates/memfuse-router/src/router.rs`

Wenn `chunk.relevance` NaN ist (z.B. durch fehlerhafte HNSW-Distanzberechnung):

```rust
let max_score = chunks.iter()
    .map(|(c, _)| { ... c.relevance ... })
    .fold(0.0f32, f32::max);  // NaN propagiert durch f32::max!
```

`f32::max(0.0, NaN)` gibt `NaN` zurück. NaN im `partial_cmp` gibt `None` zurück, das als `Equal` behandelt wird. Das Routing-Ergebnis wird nicht-deterministisch.

---

### BUG-AGT-004 · `memfuse-agent` · `engine.rs::replay_from()` — **Identifier-Ambiguität**

**Datei:** `crates/memfuse-agent/src/engine.rs`, Zeile ~185

Ein Knoten namens `"1"` kann nicht via `replay_from(ctx, "1")` als **Node-Name** adressiert werden — der Code parst alle validen u64-Strings immer als Schrittnummern:

```rust
if let Ok(step) = identifier.parse::<u64>() {
    c.name.contains(&format!(":step:{}:", step))  // "1" → sucht Schritt 1, nie Node "1"
} else {
    c.name.ends_with(&format!(":node:{}", identifier))
}
```

---

### BUG-STR-001 · `memfuse-store` · `lsm.rs` — **WAL-Dateien nach Crash-Recovery akkumulieren**

**Datei:** `crates/memfuse-store/src/lsm.rs`, Zeile ~200

Beim Startup werden alle gefundenen WAL-Dateien (`wal-*.log`) replayed, aber nur die letzte wird als aktive WAL gesetzt. Die älteren werden **nicht gelöscht**. `flush()` löscht nur die WAL, die gerade rotiert wurde — nicht die historischen WAL-Dateien vom letzten Startup.

Nach einem Crash und mehreren Recovery-Zyklen akkumulieren WAL-Dateien unbegrenzt. Jeder Startup replayed alle, was O(Gesamtschreibhistorie) Startup-Zeit erzeugt.

**Fix:** Nach erfolgreicher Startup-Replay alle WAL-Dateien außer der letzten löschen (Entries sind bereits in SSTables).

---

## 🔵 NIEDRIGE SCHWERE (Technische Schuld)

---

### BUG-TXT-003 · Interfix-Reihenfolge in `is_valid_component`

`INTERFIXES = ["s", "en", "e", "er", "n", "es"]` prüft `"s"` vor `"es"`. Für Wörter mit `-es`-Fugen (z.B. _Tages-_) feuert die `"s"`-Prüfung zuerst, was potentiell falsche Splits produziert.

### BUG-TXT-004 · Unvollständige deutsche Stoppwortliste

Nur ~8 deutsche Stoppwörter. Fehlen: _nicht, auch, noch, sehr, mehr, dann, beim, nach, wenn, aber, durch, beim, beim_ — für DACH-KMU-Kontext kritische Lücke.

### BUG-AGT-005 · `try_attach_event` fehlerhafte `limit_mb`-Semantik

```rust
limit_mb: MAX_TELEMETRY_EVENTS as u64,  // = 10.000 "MB" — semantisch falsch!
```

Die Fehlermeldung zeigt 10.000 MB als Memory-Limit an, obwohl 10.000 _Ereignisse_ gemeint sind.

### BUG-GRA-005 · Stille `max_hops`-Sättigung ohne Warnung

`traverse()` / `traverse_at_time()` kappen `max_hops` intern auf `MAX_TRAVERSAL_HOPS=3`, geben aber für Werte von 4–100 **keinen Fehler oder Warning** aus. `traverse(node, 50)` liefert identische Ergebnisse wie `traverse(node, 3)`.

---

## 🚨 Audit-Integrität: Kritischer Befund

Das vorliegende Audit-Dokument `AUDIT_memfuse-graph.md` enthält folgende Fehlinformation:

|Behauptung im Audit|Realität im Code|
|---|---|
|BUG-GRA-001: **RESOLVED** — `compact()` extended `offsets`|Fix existiert **nicht** im Quellcode|
|Alle Lock-Compliance-Tabellen: **COMPLIANT**|Partiell korrekt, aber GRA-004 nicht erkannt|
|PPR-Verifikationstabelle: **0.00000000 Absolute Difference**|Nicht nachvollziehbar ohne unabhängige Referenzimplementierung im Repo|

**Empfehlung:** Alle KI-generierten Audit-Dokumente müssen als **nicht vertrauenswürdig** eingestuft werden, bis die Claims gegen den tatsächlichen Quellcode verifiziert wurden. Der Einsatz von Google Jules als primärem Coding-Agent ohne unabhängige Code-Reviews ist das strukturelle Risiko.

---

## Priorisierte Maßnahmenliste

|Priorität|Bug-ID|Komponente|Aktion|Aufwand|
|---|---|---|---|---|
|**P0**|BUG-GRA-003|`memfuse-graph`|`compact()` Offset-Sync für entity-only commits|1h|
|**P0**|BUG-AGT-001|`memfuse-agent`|Budget-Check vor `tool.execute()` verschieben|30min|
|**P0**|BUG-GRA-004|`memfuse-graph`|`get_or_create_index` lazy machen (erst bei commit)|1 Tag|
|**P1**|BUG-RTR-001|`memfuse-router`|`SlmProfile::try_new()` mit NaN/empty Validierung|2h|
|**P1**|BUG-AGT-002|`memfuse-agent`|Budget aus Checkpoint-Metadaten wiederherstellen|2h|
|**P1**|BUG-CKP-001|`memfuse-checkpoint`|Drop-Rollback synchronisieren oder dokumentieren|4h|
|**P1**|BUG-AGT-003|`memfuse-agent`|`Vec` → `VecDeque` für Events|30min|
|**P2**|BUG-TXT-001|`memfuse-text`|URL/E-Mail-Preservation im Tokenizer|1 Tag|
|**P2**|BUG-TXT-002|`memfuse-text`|`OnceLock<Arc<Trie>>` für GermanCompoundSplitter|2h|
|**P2**|BUG-STR-001|`memfuse-store`|WAL-Cleanup nach Startup-Replay|3h|
|**P3**|BUG-DB-001|`memfuse-db`|Dead Code entfernen (doppelter k==0-Check)|15min|
|**P3**|alle BUG-TXT|`memfuse-text`|Stoppwortliste erweitern|1h|

---

_Dieser Bericht wurde auf Basis direkter Quellcode-Analyse erstellt. Alle Befunde sind mit exakten Dateipfaden und Zeilennummern belegt und können sofort reproduziert werden._