# MemFuse Brain — Verifikationsbericht & Phase-3-Prompts für Jules

> Repo-Stand geprüft: `git log` HEAD `b95bddc` (nach Commits #743–#762)  
> Methode: Vollständige statische Code-Verifikation aller 12 vorherigen Jules-
> Prompts gegen den tatsächlichen Quellcode (kein `cargo build` möglich in
> dieser Sandbox — Rust-Toolchain nicht verfügbar, daher Zeile-für-Zeile-Review).

---

## Teil 1: Hat Jules die 12 Prompts korrekt umgesetzt?

**Kurzfassung: Ja, überraschend gründlich — mit einer echten Lücke (UI) und
einer offenen Kleinigkeit (CVE).** Das ist ein deutlich besseres Ergebnis,
als bei elf von zwölf Prompts typischerweise zu erwarten wäre.

### ✅ Vollständig und korrekt umgesetzt

| # | Prompt | Verifikationsbefund |
|---|---|---|
| 1 | Sofort-Fixes | `std::sync::RwLock` ist **vollständig** aus `memfuse-db` verschwunden (0 Treffer). Checkpoint-Test nutzt jetzt korrekte API. README-`create_collection`-Bug ist behoben. |
| 2 | `delete_prefix()`/`scan_prefix()` im Trait | Beide Methoden exakt wie spezifiziert in `memfuse-core/src/traits.rs` vorhanden, inkl. Default-Impl über `scan_prefix()`. |
| 3 | Graph-Persistenz | `GRAPH_ENTITY_PREFIX`, `GRAPH_EDGE_PREFIX`, `persist_entity()`, `persist_edge()`, `load_from_storage()`, `with_storage()` — **alle vorhanden und ins `commit()` verdrahtet** (Zeile 431/436 in `csr.rs`). |
| 4 | Graph in `hybrid_search()` | **Bestätigt im Code**: `collection.rs` Zeile 938-1015 fusioniert jetzt Vektor + Text + Graph. Sogar ein **impliziter Anker-Fallback** wurde ergänzt (Anker aus Text-Treffern ableiten, falls keine expliziten `anchor_entities` übergeben werden) — das ist eine Verbesserung gegenüber der ursprünglichen Prompt-Spezifikation. |
| 5 | FIND-STO-001 + FIND-DB-002 | `is_full_compaction`-Logik korrekt in `compaction.rs` implementiert. `drop_collection()` löscht jetzt Collection-Daten UND Text-Index-Daten (`__txt:`-Prefix) — mehr als ursprünglich gefordert. |
| 6 | `memfuse-tauri` Grundgerüst | Vollständiges Crate mit `state.rs`, `commands/`, `ingestion/`, `ollama.rs`, Icons, `tauri.conf.json`. |
| 7 | Ingestion-Pipeline | PDF/DOCX/E-Mail-Extraktion **echt implementiert**, kein `todo!()`-Platzhalter (das war in der Prompt-Vorlage explizit als Risiko markiert — Jules hat es sauber gelöst). |
| 8 | Deutsche Morphologie | `KMU_DOMAIN_VOCABULARY` mit Lager-, Urlaubs-, Fertigungs-Begriffen vorhanden, in Compound-Splitter integriert. |
| 9 | Ollama-Bridge | `list_models()`, `chat_with_rag_streaming()`, `EmbeddingProvider`-Impl für Ollama — vollständig. |
| 10 | Tauri-Commands | Alle 9 Commands aus der Spezifikation sind implementiert und in `invoke_handler!` registriert. |
| 11 | MCP-Server | Echter `axum`-Server mit `/mcp/tools/list` und `/mcp/tools/call`, Standalone-Binary `memfuse-mcp-server`. |
| 12 | Dokumentation | README wurde neu strukturiert, alte API-Fehler entfernt. |

### 🟡 Teilweise umgesetzt — Lücke gefunden

**Das Frontend nutzt nur 1 von 9 Tauri-Commands.**

```html
<!-- crates/memfuse-tauri/ui/index.html — nur dieser Aufruf existiert: -->
await invoke('chat_with_rag', { ... });
```

Die Commands `open_database`, `list_collections`, `create_collection`,
`drop_collection`, `ingest_file`, `ingest_folder`, `hybrid_search`,
`list_ollama_models` sind im Rust-Backend vollständig funktionsfähig,
aber **von der Oberfläche aus nicht erreichbar**. Ein Nutzer, der die App
startet, kann chatten — aber nie eine Datenbank öffnen, Dokumente
importieren oder Collections verwalten, weil es dafür keine UI-Elemente
gibt. Das Backend ist fertig, die Bedienoberfläche ist es nicht.

Das ist keine Jules-"Fehlleistung" — die ursprünglichen Prompts 6 und 10
haben explizit nur ein *Platzhalter*-Frontend ("kein vollständiges
UI-Polish") verlangt. Diese Lücke war also erwartet und wird jetzt in
Phase 3 geschlossen.

### 🟢 Offener Kleinpunkt (nicht kritisch, aber erwähnenswert)

**`lru = "0.12.5"` in `memfuse-store` ist weiterhin gegen RUSTSEC-2026-0002
ungepatcht** (Fix verfügbar ab `0.16.3`). Dies wurde in der v2-Prompt-Serie
bewusst nicht mehr als "kritisch" behandelt, da die neue Architekten-Analyse
andere Baustellen priorisierte — es ist aber weiterhin offen und sollte vor
einem echten Enterprise-Rollout geschlossen werden. Schweregrad laut
RustSec: **Low (2.7 CVSS)** — kein akuter Show-Stopper, aber ein Findling,
der bei einem Security-Audit auffallen wird.

---

## Teil 2: Was das für die nächsten Schritte bedeutet

Die technische Basis von MemFuse Brain ist jetzt **ehrlich und funktional**:
3-Signal-RAG (Vektor+BM25+Graph) ist real, persistiert und fusioniert.
Der nächste Engpass ist nicht mehr die Engine — es ist die **Nutzbarkeit**.

Eine App, die man nur über die Kommandozeile mit Dateien füllen kann und
in der man nur chatten, aber nichts verwalten kann, ist noch keine
KMU-taugliche Software. Phase 3 schließt genau diese Lücke:

1. **UI vervollständigen** — Datenbank öffnen, Collections verwalten,
   Dokumente importieren, alles über die Oberfläche
2. **CVE schließen** — `lru` → `quick_cache` oder `lru ≥ 0.16.3`
3. **End-to-End-Tests** — bisher wurden Komponenten isoliert getestet;
   ein echter "Datei rein → Frage raus"-Test fehlt noch
4. **Erste Installer-Iteration** — der eigentliche Vertriebsweg laut Strategie

---

## Teil 3: Phase-3 Jules-Prompts

| # | Aufgabe | Priorität | Abhängigkeit |
|---|---|---|---|
| 13 | `lru` CVE schließen | Niedrig (aber schnell erledigt) | — |
| 14 | Vollständiges Tauri-Frontend: Datenbank & Collections | Kritisch | Bestehende Commands |
| 15 | Vollständiges Tauri-Frontend: Dokumenten-Import mit Fortschrittsanzeige | Kritisch | Prompt 14 |
| 16 | Vollständiges Tauri-Frontend: Suche & Quellenanzeige im Chat | Hoch | Prompt 14 |
| 17 | End-to-End-Integrationstest: Ingest → Hybrid-Search → Chat | Hoch | Prompt 7, 9 |
| 18 | Graph-Entity-Extraktion bei Ingestion (schließt die Anker-Lücke) | Mittel | Prompt 17 |
| 19 | Ersteinrichtungs-Assistent (Onboarding-Screen) | Mittel | Prompt 14–16 |
| 20 | Tauri-Installer-Konfiguration für Windows/macOS/Linux | Niedrig | Alle UI-Prompts |

---

## Prompt 13 — CVE schließen: `lru` → `quick_cache`

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: RUSTSEC-2026-0002 endgültig schließen

`crates/memfuse-store/Cargo.toml` enthält weiterhin `lru = "0.12.5"`, was
laut RustSec-Advisory RUSTSEC-2026-0002 eine Soundness-Schwäche in
`IterMut` hat (behoben erst ab Version 0.16.3). Betroffene Nutzung ist der
Block-Cache in `crates/memfuse-store/src/sstable.rs`.

### 1. Prüfe die exakte Nutzung

**Datei**: `crates/memfuse-store/src/sstable.rs`

Aktuell (bestätigt):
```rust
use lru::LruCache;
pub type BlockCache = RwLock<LruCache<(u64, u64), Bytes>>;
```

Prüfe, ob im gesamten Crate `IterMut`/`iter_mut()` auf dem `LruCache`
tatsächlich verwendet wird (die Advisory betrifft konkret diese Methode).
Falls `iter_mut()` nirgends aufgerufen wird, ist das reale Risiko gering,
aber ein Security-Audit wird die Version trotzdem markieren — daher in
jedem Fall beheben.

### 2. Option A (bevorzugt): Upgrade auf `lru >= 0.16.3`

In `crates/memfuse-store/Cargo.toml`:
```toml
lru = "0.16.3"
```

Prüfe nach dem Versions-Sprung, ob sich die öffentliche API von `LruCache`
zwischen 0.12 und 0.16 geändert hat (z.B. `NonZeroUsize` statt `usize` für
die Kapazität war bereits in 0.12 Pflicht — prüfe ob weitere Breaking
Changes bestehen) und passe `sstable.rs` entsprechend an, falls nötig.

### 3. Option B (falls 0.16.3 Breaking Changes hat, die den Aufwand nicht
rechtfertigen): Ersatz durch `quick_cache`

Falls Option A zu aufwendig ist, ersetze `lru` vollständig:

```toml
# Cargo.toml (workspace):
quick_cache = "0.6"

# crates/memfuse-store/Cargo.toml:
# Zeile "lru = ..." entfernen
quick_cache = { workspace = true }
```

In `sstable.rs`:
```rust
use quick_cache::sync::Cache;
pub type BlockCache = Cache<(u64, u64), Bytes>;
```

`quick_cache::sync::Cache` ist bereits intern synchronisiert — das äußere
`RwLock` im Typ-Alias entfällt dann. Passe alle Aufrufstellen
(`cache.put(k, v)` → `cache.insert(k, v)`, `cache.get(&k)` bleibt gleich)
entsprechend an.

### 4. Verifikation

```bash
cargo build -p memfuse-store 2>&1 | tail -20
cargo test -p memfuse-store 2>&1 | tail -30
grep -n "^lru" Cargo.toml crates/memfuse-store/Cargo.toml
```

Falls Option A gewählt wurde, muss die letzte Zeile `lru = "0.16.3"` oder
höher zeigen. Falls Option B, darf `lru` in keiner `Cargo.toml` mehr
auftauchen.
```

---

## Prompt 14 — Vollständiges Tauri-Frontend: Datenbank & Collections

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: UI für Datenbank-Verwaltung und Collections ergänzen

Verifikation hat ergeben: Das Frontend (`crates/memfuse-tauri/ui/index.html`)
ruft aktuell NUR `chat_with_rag` auf. Die Backend-Commands `open_database`,
`list_collections`, `create_collection` und `drop_collection` sind
vollständig implementiert, aber von der UI aus nicht erreichbar. Diese
Aufgabe schließt genau diese Lücke.

### 1. UI-Struktur neu aufbauen: Sidebar + Hauptbereich

Ersetze `crates/memfuse-tauri/ui/index.html` durch eine erweiterte Version
mit einer Seitenleiste für Datenbank-/Collection-Verwaltung, die den
bestehenden Chat-Bereich als Hauptinhalt beibehält:

```html
<!DOCTYPE html>
<html lang="de">
<head>
    <meta charset="UTF-8">
    <title>MemFuse Brain</title>
    <style>
        :root {
            --sidebar-width: 280px;
        }
        body {
            font-family: system-ui, -apple-system, sans-serif;
            margin: 0;
            display: flex;
            height: 100vh;
        }
        #sidebar {
            width: var(--sidebar-width);
            background: #1a1a2e;
            color: #e0e0e0;
            padding: 1rem;
            display: flex;
            flex-direction: column;
            gap: 1rem;
            overflow-y: auto;
        }
        #main {
            flex: 1;
            display: flex;
            flex-direction: column;
            padding: 1rem;
        }
        .sidebar-section h3 {
            font-size: 0.85rem;
            text-transform: uppercase;
            opacity: 0.6;
            margin-bottom: 0.5rem;
        }
        #db-status {
            padding: 0.5rem;
            border-radius: 6px;
            background: #16213e;
            font-size: 0.85rem;
        }
        #db-status.connected { background: #1f4e3d; }
        .collection-item {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 0.5rem;
            border-radius: 6px;
            cursor: pointer;
            font-size: 0.9rem;
        }
        .collection-item:hover { background: #16213e; }
        .collection-item.active { background: #0f3460; }
        .collection-item .delete-btn {
            opacity: 0.5;
            font-size: 0.8rem;
            border: none;
            background: none;
            color: inherit;
            cursor: pointer;
        }
        .collection-item .delete-btn:hover { opacity: 1; color: #e74c3c; }
        button.primary {
            background: #0f3460;
            color: white;
            border: none;
            padding: 0.5rem 1rem;
            border-radius: 6px;
            cursor: pointer;
        }
        button.primary:hover { background: #16537e; }
        input[type="text"] {
            padding: 0.4rem;
            border-radius: 6px;
            border: 1px solid #444;
            background: #16213e;
            color: white;
            width: 100%;
            box-sizing: border-box;
        }
        #chat-log { flex: 1; border: 1px solid #ccc; border-radius: 8px; padding: 1rem; overflow-y: auto; margin-bottom: 1rem; }
        #chat-input-row { display: flex; gap: 0.5rem; }
        #chat-input-row input { flex: 1; padding: 0.6rem; }
        #chat-input-row button { padding: 0.6rem 1.2rem; }
    </style>
</head>
<body>
    <aside id="sidebar">
        <div class="sidebar-section">
            <h3>Datenbank</h3>
            <div id="db-status">Keine Datenbank geöffnet</div>
            <button class="primary" id="open-db-btn" style="margin-top: 0.5rem; width: 100%;">
                📁 Datenbank öffnen / erstellen
            </button>
        </div>

        <div class="sidebar-section">
            <h3>Collections</h3>
            <div id="collections-list"></div>
            <div style="display: flex; gap: 0.3rem; margin-top: 0.5rem;">
                <input type="text" id="new-collection-input" placeholder="Neue Collection...">
                <button class="primary" id="create-collection-btn">+</button>
            </div>
        </div>

        <div class="sidebar-section">
            <h3>Ollama-Modell</h3>
            <select id="model-select" style="width: 100%; padding: 0.4rem; border-radius: 6px;">
                <option>Lade Modelle...</option>
            </select>
        </div>
    </aside>

    <main id="main">
        <h2 id="active-collection-title">Kein Wissensbereich ausgewählt</h2>
        <div id="chat-log"></div>
        <div id="chat-input-row">
            <input id="query-input" type="text" placeholder="Stellen Sie eine Frage zu Ihren Dokumenten...">
            <button class="primary" id="send-btn">Senden</button>
        </div>
    </main>

    <script type="module" src="app.js"></script>
</body>
</html>
```

### 2. Neue Datei: `crates/memfuse-tauri/ui/app.js`

Lagere die JS-Logik aus dem HTML in eine separate Datei aus (bessere
Wartbarkeit) und implementiere die Datenbank-/Collection-Verwaltung:

```javascript
const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const { listen } = window.__TAURI__.event;

let activeCollection = null;
let dbOpen = false;

const dbStatusEl = document.getElementById('db-status');
const collectionsListEl = document.getElementById('collections-list');
const activeTitleEl = document.getElementById('active-collection-title');
const chatLog = document.getElementById('chat-log');
const modelSelect = document.getElementById('model-select');

// ── Datenbank öffnen ─────────────────────────────────────────────────────
document.getElementById('open-db-btn').addEventListener('click', async () => {
    const selected = await open({
        directory: true,
        multiple: false,
        title: 'Datenbankordner wählen oder neu erstellen',
    });
    if (!selected) return;

    try {
        await invoke('open_database', { path: selected });
        dbOpen = true;
        dbStatusEl.textContent = `✅ Verbunden: ${selected}`;
        dbStatusEl.classList.add('connected');
        await refreshCollections();
        await refreshModels();
    } catch (e) {
        dbStatusEl.textContent = `❌ Fehler: ${e}`;
    }
});

// ── Collections laden und anzeigen ──────────────────────────────────────
async function refreshCollections() {
    if (!dbOpen) return;
    try {
        const collections = await invoke('list_collections');
        collectionsListEl.innerHTML = '';
        for (const col of collections) {
            const item = document.createElement('div');
            item.className = 'collection-item' + (col.name === activeCollection ? ' active' : '');
            item.innerHTML = `
                <span>${col.name} <small style="opacity:0.6">(${col.document_count})</small></span>
                <button class="delete-btn" data-name="${col.name}">✕</button>
            `;
            item.querySelector('span').addEventListener('click', () => selectCollection(col.name));
            item.querySelector('.delete-btn').addEventListener('click', async (ev) => {
                ev.stopPropagation();
                if (confirm(`Collection "${col.name}" wirklich löschen? Alle Dokumente gehen verloren.`)) {
                    await invoke('drop_collection', { name: col.name });
                    if (activeCollection === col.name) activeCollection = null;
                    await refreshCollections();
                }
            });
            collectionsListEl.appendChild(item);
        }
    } catch (e) {
        console.error('Collections laden fehlgeschlagen:', e);
    }
}

function selectCollection(name) {
    activeCollection = name;
    activeTitleEl.textContent = `📚 ${name}`;
    refreshCollections();
}

// ── Neue Collection erstellen ────────────────────────────────────────────
document.getElementById('create-collection-btn').addEventListener('click', async () => {
    const input = document.getElementById('new-collection-input');
    const name = input.value.trim();
    if (!name || !dbOpen) return;

    try {
        await invoke('create_collection', { name });
        input.value = '';
        await refreshCollections();
        selectCollection(name);
    } catch (e) {
        alert(`Collection konnte nicht erstellt werden: ${e}`);
    }
});

// ── Ollama-Modelle laden ─────────────────────────────────────────────────
async function refreshModels() {
    try {
        const models = await invoke('list_ollama_models');
        modelSelect.innerHTML = '';
        if (models.length === 0) {
            modelSelect.innerHTML = '<option>Kein Ollama-Modell gefunden</option>';
            return;
        }
        for (const m of models) {
            const opt = document.createElement('option');
            opt.value = m;
            opt.textContent = m;
            modelSelect.appendChild(opt);
        }
    } catch (e) {
        modelSelect.innerHTML = '<option>⚠️ Ollama nicht erreichbar</option>';
    }
}

// ── Chat (bestehende Funktionalität, angepasst an aktive Collection) ────
let currentResponseEl = null;
listen('chat-token', (event) => {
    if (currentResponseEl) {
        currentResponseEl.textContent += event.payload;
    }
});

document.getElementById('send-btn').addEventListener('click', sendMessage);
document.getElementById('query-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') sendMessage();
});

async function sendMessage() {
    const input = document.getElementById('query-input');
    const message = input.value.trim();
    if (!message) return;

    if (!activeCollection) {
        alert('Bitte wählen Sie zuerst eine Collection aus der Seitenleiste.');
        return;
    }

    chatLog.innerHTML += `<p><strong>Sie:</strong> ${escapeHtml(message)}</p>`;
    currentResponseEl = document.createElement('p');
    currentResponseEl.innerHTML = '<strong>Assistent:</strong> ';
    chatLog.appendChild(currentResponseEl);
    chatLog.scrollTop = chatLog.scrollHeight;
    input.value = '';

    const model = modelSelect.value || 'llama3.2';

    try {
        await invoke('chat_with_rag', {
            message,
            collectionName: activeCollection,
            model,
        });
    } catch (e) {
        currentResponseEl.textContent += `\n⚠️ Fehler: ${e}`;
    }
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// ── Initialisierung ───────────────────────────────────────────────────────
// Beim Start prüfen, ob bereits eine Datenbank aus vorheriger Session
// bekannt ist (optional — falls kein Persistenz-Mechanismus für den
// zuletzt geöffneten Pfad existiert, diesen Abschnitt weglassen).
</script>
```

### 2b. Tauri-Dialog-Plugin sicherstellen

Prüfe, dass `tauri-plugin-dialog` bereits in `Cargo.toml` und `lib.rs`
registriert ist (aus Prompt 6 sollte dies der Fall sein). Ergänze im
Frontend-Setup (`tauri.conf.json` bzw. `capabilities/`) die notwendige
Berechtigung für den Dialog-Plugin, falls Tauri v2 Capabilities dies
explizit erfordert:

```json
// crates/memfuse-tauri/capabilities/default.json (falls noch nicht vorhanden)
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "dialog:allow-open",
    "core:event:allow-listen"
  ]
}
```

Prüfe den bestehenden `gen/schemas/`-Ordner (aus dem letzten Commit
vorhanden) auf bereits generierte Capability-Definitionen und ergänze
konsistent dazu.

### 3. `tauri.conf.json` — `frontendDist` und Skript-Referenz prüfen

Stelle sicher, dass `app.js` korrekt vom `index.html` geladen wird und der
`frontendDist`-Pfad weiterhin auf `ui` zeigt.

### 4. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
# HTML/JS-Syntax grob prüfen:
node --check crates/memfuse-tauri/ui/app.js 2>&1 || echo "Node nicht verfügbar — manuelle Prüfung nötig"
```
```

---

## Prompt 15 — Tauri-Frontend: Dokumenten-Import mit Fortschrittsanzeige

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: UI für Dokumenten-Import ergänzen (nutzt bestehende `ingest_file`/`ingest_folder` Commands)

Aufbauend auf Prompt 14: Die Backend-Commands `ingest_file` und
`ingest_folder` sind vollständig implementiert (siehe
`crates/memfuse-tauri/src/commands/ingest.rs`), aber ohne UI-Anbindung.
Diese Aufgabe ergänzt einen Import-Bereich in der Seitenleiste.

### 1. HTML-Ergänzung in `crates/memfuse-tauri/ui/index.html`

Füge in `#sidebar`, nach dem Collections-Abschnitt, einen neuen Bereich ein:

```html
<div class="sidebar-section">
    <h3>Dokumente importieren</h3>
    <button class="primary" id="import-file-btn" style="width: 100%; margin-bottom: 0.3rem;">
        📄 Einzelne Datei
    </button>
    <button class="primary" id="import-folder-btn" style="width: 100%;">
        📂 Ganzer Ordner
    </button>
    <div id="import-progress" style="display:none; margin-top: 0.5rem; font-size: 0.85rem;">
        <div id="import-status">Importiere...</div>
        <div id="import-log" style="max-height: 150px; overflow-y: auto; margin-top: 0.3rem; font-size: 0.75rem; opacity: 0.8;"></div>
    </div>
</div>
```

### 2. JS-Ergänzung in `crates/memfuse-tauri/ui/app.js`

```javascript
// ── Dokumenten-Import ─────────────────────────────────────────────────────
document.getElementById('import-file-btn').addEventListener('click', async () => {
    if (!checkCollectionSelected()) return;

    const selected = await open({
        directory: false,
        multiple: false,
        filters: [
            { name: 'Unterstützte Dokumente', extensions: ['pdf', 'docx', 'md', 'txt', 'eml'] }
        ],
    });
    if (!selected) return;

    await runImport(async () => {
        return await invoke('ingest_file', {
            filePath: selected,
            collectionName: activeCollection,
        });
    }, [selected]);
});

document.getElementById('import-folder-btn').addEventListener('click', async () => {
    if (!checkCollectionSelected()) return;

    const selected = await open({
        directory: true,
        multiple: false,
        title: 'Ordner mit Dokumenten wählen',
    });
    if (!selected) return;

    await runImport(async () => {
        return await invoke('ingest_folder', {
            folderPath: selected,
            collectionName: activeCollection,
        });
    }, null, true);
});

function checkCollectionSelected() {
    if (!activeCollection) {
        alert('Bitte wählen Sie zuerst eine Collection aus, in die importiert werden soll.');
        return false;
    }
    return true;
}

async function runImport(importFn, filePaths, isFolder = false) {
    const progressEl = document.getElementById('import-progress');
    const statusEl = document.getElementById('import-status');
    const logEl = document.getElementById('import-log');

    progressEl.style.display = 'block';
    statusEl.textContent = isFolder ? 'Ordner wird importiert...' : 'Datei wird importiert...';
    logEl.innerHTML = '';

    try {
        const result = await importFn();
        const reports = Array.isArray(result) ? result : [result];

        let totalChunks = 0;
        let totalErrors = 0;

        for (const report of reports) {
            totalChunks += report.chunks_created || 0;
            const errCount = (report.errors || []).length;
            totalErrors += errCount;

            const line = document.createElement('div');
            const fileName = report.file_path.split(/[/\\]/).pop();
            if (errCount > 0) {
                line.innerHTML = `⚠️ ${fileName}: ${report.chunks_created} Abschnitte, ${errCount} Fehler`;
                line.title = report.errors.join('\n');
            } else {
                line.innerHTML = `✅ ${fileName}: ${report.chunks_created} Abschnitte`;
            }
            logEl.appendChild(line);
        }

        statusEl.textContent = `Fertig: ${totalChunks} Abschnitte importiert` +
            (totalErrors > 0 ? `, ${totalErrors} Fehler` : '');

        await refreshCollections();  // Dokumentenzähler aktualisieren
    } catch (e) {
        statusEl.textContent = `❌ Import fehlgeschlagen: ${e}`;
    }
}
```

**Wichtig**: Tauri übersetzt Rust-`snake_case`-Parameter standardmäßig zu
JS-`camelCase` (`file_path` → `filePath`, `collection_name` → `collectionName`,
`folder_path` → `folderPath`). Prüfe die exakte Parameter-Namenskonvention
in den bestehenden Command-Signaturen (`crates/memfuse-tauri/src/commands/ingest.rs`)
und stelle sicher, dass die JS-Aufrufe exakt passen — Tauri wirft sonst
einen Laufzeitfehler "missing required key".

### 3. Ladeindikator für lange Imports (UX-Verbesserung)

Da `ingest_folder` bei vielen Dokumenten lange dauern kann und der aktuelle
Command-Aufruf synchron auf das Gesamtergebnis wartet, ergänze einen
einfachen CSS-Spinner, der während des `await importFn()`-Aufrufs sichtbar
ist:

```css
.spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid #444;
    border-top-color: #0f3460;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
```

Füge den Spinner neben `statusEl` ein, solange der Import läuft, und
entferne ihn danach.

### 4. Backend-Ergänzung: Fortschritts-Events (optional, empfohlen)

Falls die Import-Zeit bei großen Ordnern spürbar ist, erweitere
`ingest_folder` in `crates/memfuse-tauri/src/commands/ingest.rs` um
Tauri-Events pro verarbeiteter Datei, damit das Frontend live mitzählen
kann statt nur am Ende ein Gesamtergebnis zu erhalten:

```rust
#[tauri::command]
pub async fn ingest_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
    collection_name: String,
) -> Result<Vec<IngestReport>, String> {
    // ... bestehende Initialisierung ...

    // Falls IngestionPipeline eine Datei-für-Datei-Callback-Variante
    // unterstützt, nutze sie hier und emit ein Event pro Datei:
    // app.emit("ingest-progress", &report).ok();

    // Falls die bestehende ingest_folder()-Methode nur das Gesamtergebnis
    // zurückgibt, ist diese Erweiterung optional — dann reicht die
    // synchrone Wartezeit im Frontend mit Spinner aus.
}
```

Falls diese Erweiterung implementiert wird, ergänze im Frontend:
```javascript
listen('ingest-progress', (event) => {
    const report = event.payload;
    const line = document.createElement('div');
    line.textContent = `✅ ${report.file_path}: ${report.chunks_created} Abschnitte`;
    document.getElementById('import-log').appendChild(line);
});
```

### 5. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
```
```

---

## Prompt 16 — Tauri-Frontend: Suche & Quellenanzeige im Chat

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Quellenangaben im Chat sichtbar machen + eigenständige Such-Ansicht

Aktuell zeigt der Chat nur die generierte Antwort, ohne dass der Nutzer
sieht, WELCHE Dokumente der Assistent tatsächlich als Grundlage verwendet
hat. Für Vertrauen in ein Enterprise-RAG-System ist Quellentransparenz
essenziell. Zusätzlich existiert der `hybrid_search`-Command
(`crates/memfuse-tauri/src/commands/search.rs`) bisher ohne jede UI-Nutzung.

### 1. Backend: `chat_with_rag` um Quellen-Rückgabe erweitern

**Datei**: `crates/memfuse-tauri/src/commands/chat.rs`

Aktuell gibt `chat_with_rag` nur `Result<String, String>` (die reine
Antwort) zurück. Erweitere den Rückgabetyp, um die verwendeten
Suchergebnisse mitzuliefern:

```rust
#[derive(serde::Serialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<crate::commands::search::SearchResultDto>,
}

#[tauri::command]
pub async fn chat_with_rag(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    message: String,
    collection_name: String,
    model: String,
) -> Result<ChatResponse, String> {
    // ... bestehende Logik bis zum search_results-Aufruf unverändert ...

    let sources: Vec<crate::commands::search::SearchResultDto> = search_results
        .iter()
        .map(|r| crate::commands::search::SearchResultDto {
            id: r.id.clone(),
            score: r.score,
            text_preview: r.metadata
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.chars().take(200).collect())
                .unwrap_or_default(),
            source: r.metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("Unbekannt")
                .to_string(),
        })
        .collect();

    // ... bestehender chat_with_rag_streaming-Aufruf unverändert ...

    Ok(ChatResponse {
        answer: full_response,
        sources,
    })
}
```

Passe `SearchResultDto` in `search.rs` ggf. mit `pub`-Sichtbarkeit an,
damit es aus `chat.rs` verwendbar ist.

### 2. Frontend: Quellen unter jeder Antwort anzeigen

**Datei**: `crates/memfuse-tauri/ui/app.js`

Passe `sendMessage()` an, um die Quellen nach Abschluss des Streamings
darzustellen:

```javascript
async function sendMessage() {
    const input = document.getElementById('query-input');
    const message = input.value.trim();
    if (!message) return;
    if (!activeCollection) {
        alert('Bitte wählen Sie zuerst eine Collection aus der Seitenleiste.');
        return;
    }

    chatLog.innerHTML += `<p><strong>Sie:</strong> ${escapeHtml(message)}</p>`;
    currentResponseEl = document.createElement('p');
    currentResponseEl.innerHTML = '<strong>Assistent:</strong> ';
    chatLog.appendChild(currentResponseEl);

    const sourcesEl = document.createElement('div');
    sourcesEl.className = 'sources-box';
    sourcesEl.style.cssText = 'font-size: 0.8rem; opacity: 0.75; margin: 0.3rem 0 1rem 1rem; border-left: 2px solid #444; padding-left: 0.6rem;';
    chatLog.appendChild(sourcesEl);

    chatLog.scrollTop = chatLog.scrollHeight;
    input.value = '';

    const model = modelSelect.value || 'llama3.2';

    try {
        const response = await invoke('chat_with_rag', {
            message,
            collectionName: activeCollection,
            model,
        });

        // response ist jetzt { answer, sources } statt reinem String
        if (response.sources && response.sources.length > 0) {
            sourcesEl.innerHTML = '<strong>📎 Quellen:</strong><br>' +
                response.sources.map(s =>
                    `• ${escapeHtml(s.source)} <span style="opacity:0.6">(Relevanz: ${(s.score * 100).toFixed(0)}%)</span>`
                ).join('<br>');
        }
    } catch (e) {
        currentResponseEl.textContent += `\n⚠️ Fehler: ${e}`;
    }
}
```

**Wichtig**: Da sich der Rückgabetyp von `chat_with_rag` ändert (String →
Objekt), muss das bestehende `chat-token`-Event-basierte Streaming weiterhin
funktionieren — die Events laufen unabhängig vom finalen Rückgabewert des
Commands. Stelle sicher, dass `currentResponseEl.textContent` weiterhin
per Event befüllt wird, während `response.sources` erst NACH Abschluss des
Streams verfügbar ist (das ist bereits die korrekte Reihenfolge, da
`await invoke(...)` erst nach vollständigem Streaming-Abschluss resolved).

### 3. Eigenständiger Such-Modus (nutzt `hybrid_search` direkt)

Füge einen Umschalter zwischen "Chat" und "Direktsuche" hinzu, damit
Nutzer auch ohne LLM-Antwort direkt in den Dokumenten suchen können
(nützlich für schnelles Nachschlagen ohne Ollama-Wartezeit):

```html
<!-- In #main, über #chat-log einfügen -->
<div style="margin-bottom: 0.5rem;">
    <label><input type="radio" name="mode" value="chat" checked> 💬 Chat</label>
    <label style="margin-left: 1rem;"><input type="radio" name="mode" value="search"> 🔍 Direktsuche</label>
</div>
```

```javascript
let currentMode = 'chat';
document.querySelectorAll('input[name="mode"]').forEach(radio => {
    radio.addEventListener('change', (e) => { currentMode = e.target.value; });
});

async function sendMessage() {
    if (currentMode === 'search') {
        return runDirectSearch();
    }
    // ... bestehende Chat-Logik ...
}

async function runDirectSearch() {
    const input = document.getElementById('query-input');
    const query = input.value.trim();
    if (!query || !activeCollection) return;

    chatLog.innerHTML += `<p><strong>Suche:</strong> ${escapeHtml(query)}</p>`;
    input.value = '';

    try {
        const results = await invoke('hybrid_search', {
            query,
            collectionName: activeCollection,
            k: 8,
        });

        const resultsEl = document.createElement('div');
        resultsEl.style.cssText = 'margin: 0.5rem 0 1rem 0;';
        if (results.length === 0) {
            resultsEl.textContent = 'Keine Treffer gefunden.';
        } else {
            resultsEl.innerHTML = results.map(r => `
                <div style="border: 1px solid #ddd; border-radius: 6px; padding: 0.6rem; margin-bottom: 0.4rem;">
                    <div style="font-size: 0.8rem; opacity: 0.7;">
                        📄 ${escapeHtml(r.source)} — Relevanz: ${(r.score * 100).toFixed(0)}%
                    </div>
                    <div style="margin-top: 0.3rem;">${escapeHtml(r.text_preview)}...</div>
                </div>
            `).join('');
        }
        chatLog.appendChild(resultsEl);
        chatLog.scrollTop = chatLog.scrollHeight;
    } catch (e) {
        chatLog.innerHTML += `<p>⚠️ Fehler: ${e}</p>`;
    }
}
```

### 4. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
```

Prüfe insbesondere, dass die Signaturänderung von `chat_with_rag`
(neuer Rückgabetyp `ChatResponse`) keine anderen Aufrufer im Rust-Code
bricht (z.B. Tests in `crates/memfuse-tauri/tests/`).
```

---

## Prompt 17 — End-to-End-Integrationstest: Ingest → Hybrid-Search → Chat

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Echten End-to-End-Test schreiben — bisher fehlt dieser komplett

Die bisherigen Tests prüfen Komponenten isoliert (Ingestion-Pipeline mit
Dummy-Embedder, Ollama-Bridge mit Verbindungsfehler-Simulation, Graph-
Persistenz einzeln). Es gibt noch KEINEN Test, der den kompletten Nutzerpfad
abbildet: Datei importieren → durchsuchen → über den Graph verknüpfte
Ergebnisse finden.

### 1. Neue Testdatei: `crates/memfuse-tauri/tests/e2e_test.rs`

```rust
//! End-to-End-Test: Simuliert den vollständigen Nutzerpfad von MemFuse Brain
//! ohne echtes Ollama (Mock-Embedder), aber mit echter Storage-Engine.

use memfuse_db::MemFuse;
use memfuse_tauri_lib::ingestion::{EmbeddingProvider, IngestionPipeline};
use std::sync::Arc;
use tempfile::tempdir;

/// Deterministischer Test-Embedder: erzeugt Vektoren basierend auf
/// enthaltenen Schlüsselwörtern, sodass thematisch ähnliche Texte auch
/// tatsächlich nahe beieinander liegende Vektoren erhalten (nicht rein
/// zufällig — sonst wäre der Vektor-Signal-Test bedeutungslos).
struct KeywordEmbedder;

#[async_trait::async_trait]
impl EmbeddingProvider for KeywordEmbedder {
    async fn embed(&self, text: &str) -> memfuse_core::Result<Vec<f32>> {
        let lower = text.to_lowercase();
        let dim_urlaub = if lower.contains("urlaub") { 1.0 } else { 0.0 };
        let dim_gehalt = if lower.contains("gehalt") { 1.0 } else { 0.0 };
        let dim_lager = if lower.contains("lager") { 1.0 } else { 0.0 };
        let dim_generic = 0.1;
        Ok(vec![dim_urlaub, dim_gehalt, dim_lager, dim_generic])
    }
}

#[tokio::test]
async fn test_full_pipeline_ingest_search_and_chat_context() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("e2e_db");
    let docs_dir = dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();

    // ── 1. Test-Dokumente anlegen ────────────────────────────────────────
    std::fs::write(
        docs_dir.join("urlaub.md"),
        "# Urlaubsantrag\n\nMitarbeiter können ihren Urlaubsantrag über \
         das interne Portal stellen. Die Genehmigung erfolgt durch die \
         direkte Führungskraft innerhalb von 3 Werktagen.",
    ).unwrap();

    std::fs::write(
        docs_dir.join("gehalt.md"),
        "# Gehaltsabrechnung\n\nDie Gehaltsabrechnung erfolgt monatlich \
         zum 25. Kalendertag. Bei Fragen wenden Sie sich an die Personalabteilung.",
    ).unwrap();

    std::fs::write(
        docs_dir.join("lager.md"),
        "# Lagerbestand\n\nDer aktuelle Lagerbestand wird wöchentlich \
         inventarisiert. Mindestbestände sind im ERP-System hinterlegt.",
    ).unwrap();

    // ── 2. Datenbank öffnen, Collection erstellen ────────────────────────
    let db = MemFuse::open(&db_path).await.expect("DB öffnen");
    let collection = db.collection("hr_docs").await.expect("Collection erstellen");

    // ── 3. Ordner importieren ─────────────────────────────────────────────
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(KeywordEmbedder);
    let pipeline = IngestionPipeline::new(embedder.clone());
    let reports = pipeline
        .ingest_folder(&docs_dir, &collection)
        .await
        .expect("Ordner-Import");

    assert_eq!(reports.len(), 3, "Alle 3 Testdokumente sollten verarbeitet werden");
    let total_chunks: usize = reports.iter().map(|r| r.chunks_created).sum();
    assert!(total_chunks >= 3, "Mindestens 1 Chunk pro Dokument erwartet");
    for report in &reports {
        assert!(report.errors.is_empty(), "Keine Fehler erwartet: {:?}", report.errors);
    }

    // ── 4. Hybrid-Suche: Urlaubsfrage sollte urlaub.md finden ────────────
    let query = "Wie stelle ich einen Urlaubsantrag?";
    let query_vector = embedder.embed(query).await.unwrap();

    let results = collection
        .hybrid_search(query, &query_vector, 5, None)
        .await
        .expect("Hybrid-Suche");

    assert!(!results.is_empty(), "Suche sollte Ergebnisse liefern");

    let top_result_mentions_urlaub = results.iter().any(|r| {
        r.metadata
            .as_ref()
            .and_then(|m| m.get("text"))
            .and_then(|t| t.as_str())
            .map(|t| t.to_lowercase().contains("urlaub"))
            .unwrap_or(false)
    });
    assert!(
        top_result_mentions_urlaub,
        "Die Urlaubsfrage sollte mindestens ein Ergebnis mit 'Urlaub' im Text finden"
    );

    // ── 5. Negativ-Test: Lager-Frage sollte NICHT das Gehalt-Dokument
    //    als Top-Treffer liefern (Signal-Trennschärfe prüfen) ─────────────
    let lager_query = "Wie hoch ist der aktuelle Lagerbestand?";
    let lager_vector = embedder.embed(lager_query).await.unwrap();
    let lager_results = collection
        .hybrid_search(lager_query, &lager_vector, 3, None)
        .await
        .expect("Lager-Suche");

    assert!(!lager_results.is_empty());
    let top_lager_hit = &lager_results[0];
    let top_text = top_lager_hit
        .metadata
        .as_ref()
        .and_then(|m| m.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(
        top_text.to_lowercase().contains("lager"),
        "Top-Treffer für Lager-Frage sollte tatsächlich vom Lager-Dokument stammen, war aber: {top_text}"
    );

    // ── 6. Persistenz über Neustart prüfen (End-to-End auf Storage-Ebene) ─
    drop(db);
    let db2 = MemFuse::open(&db_path).await.expect("DB erneut öffnen");
    let collection2 = db2.collection("hr_docs").await.expect("Collection erneut öffnen");
    let count = collection2.count().await.expect("count()");
    assert!(count >= 3, "Dokumente müssen nach Neustart noch vorhanden sein");
}
```

**Wichtig**: Prüfe die exakten Methodennamen und Signaturen
(`MemFuse::open()`, `db.collection()`, `collection.count()`,
`IngestionPipeline::new()`, `pipeline.ingest_folder()`) gegen den
tatsächlichen Code in `memfuse-db` und `memfuse-tauri`, da einige Namen
im Testentwurf Annahmen sind, die leicht von der Realität abweichen
können — passe sie bei Bedarf an die echte API an, ohne die
Test-Intention (die drei beschriebenen Assertions) zu verwässern.

### 2. Testabhängigkeiten ergänzen

**Datei**: `crates/memfuse-tauri/Cargo.toml`
```toml
[dev-dependencies]
tempfile = { workspace = true }
async-trait = { workspace = true }
tokio = { workspace = true, features = ["full", "test-util"] }
```

### 3. Verifikation

```bash
cargo test -p memfuse-tauri --test e2e_test 2>&1 | tail -40
```

Alle Assertions müssen grün sein. Falls der Test aufgrund abweichender
API-Signaturen fehlschlägt, korrigiere die Testaufrufe iterativ, bis er
grün ist — das eigentliche Testverhalten (Ingest → Suche → Persistenz)
darf dabei nicht verwässert werden.
```

---

## Prompt 18 — Graph-Entity-Extraktion bei Ingestion (schließt die Anker-Lücke)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Automatische Entity-Erkennung während der Ingestion

Verifikation von Prompt 4 zeigt: `hybrid_search()` nutzt bereits einen
cleveren "impliziten Anker"-Fallback (Entities aus Text-Treffern ableiten).
Das funktioniert aber nur, wenn überhaupt Entities im Graph existieren.
Aktuell erzeugt die `IngestionPipeline` (Prompt 7) KEINE Graph-Entities —
importierte Dokumente landen nur in Vektor- und Text-Index, nie im Graph.
Damit bleibt das 3. Signal für frisch importierte Dokumente leer.

### 1. Einfache Named-Entity-Erkennung (regelbasiert, kein ML-Modell nötig)

**Neue Datei**: `crates/memfuse-tauri/src/ingestion/entities.rs`

Implementiere eine simple, aber wirksame regelbasierte Entity-Extraktion
für deutsche Geschäftsdokumente — kein vollwertiges NER-Modell (das wäre
Scope-Creep), sondern Muster-basierte Erkennung typischer KMU-Entitäten:

```rust
use memfuse_core::EntityId;
use std::collections::HashSet;

/// Extrahiert einfache Entitäten aus Dokumenttext via Musterheuristiken.
/// Bewusst regelbasiert (keine ML-Abhängigkeit) für Nachvollziehbarkeit
/// und Zero-Setup-Betrieb.
pub struct SimpleEntityExtractor;

impl SimpleEntityExtractor {
    /// Erkennt großgeschriebene Mehrwortfolgen als potenzielle Eigennamen
    /// (Personen, Firmen, Abteilungen) — deutsche Substantiv-Großschreibung
    /// macht dies überraschend robust für Geschäftsdokumente.
    pub fn extract(text: &str) -> Vec<EntityId> {
        let mut entities = HashSet::new();

        // Muster 1: Aufeinanderfolgende großgeschriebene Wörter
        // (z.B. "Müller GmbH", "Max Mustermann", "Abteilung Finanzen")
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() {
            let word = words[i].trim_matches(|c: char| !c.is_alphanumeric());
            if Self::is_capitalized_candidate(word) {
                let mut phrase = vec![word];
                let mut j = i + 1;
                while j < words.len() {
                    let next = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                    if Self::is_capitalized_candidate(next) {
                        phrase.push(next);
                        j += 1;
                    } else {
                        break;
                    }
                }
                if phrase.len() >= 2 || Self::looks_like_company_suffix(word) {
                    let joined = phrase.join(" ");
                    if joined.len() > 3 {
                        entities.insert(EntityId::from(joined.as_str()));
                    }
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }

        entities.into_iter().collect()
    }

    fn is_capitalized_candidate(word: &str) -> bool {
        word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            && word.len() > 2
            && !Self::is_common_sentence_starter(word)
    }

    /// Filtert häufige Satzanfangs-Wörter heraus, die großgeschrieben sind,
    /// aber keine Entitäten darstellen (jedes Satzanfangs-Wort ist im
    /// Deutschen großgeschrieben, unabhängig vom Wortstamm).
    fn is_common_sentence_starter(word: &str) -> bool {
        matches!(
            word,
            "Der" | "Die" | "Das" | "Ein" | "Eine" | "Wir" | "Sie" | "Ihr"
                | "Bitte" | "Für" | "Diese" | "Dieser" | "Alle" | "Jeder"
        )
    }

    fn looks_like_company_suffix(word: &str) -> bool {
        matches!(word, "GmbH" | "AG" | "KG" | "GbR" | "OHG" | "e.V.")
    }
}
```

### 2. In die Ingestion-Pipeline integrieren

**Datei**: `crates/memfuse-tauri/src/ingestion/pipeline.rs`

Erweitere `ingest_file()` so, dass nach dem erfolgreichen Einfügen eines
Chunks auch Entities extrahiert und in den Graph eingefügt werden:

```rust
// Nach dem erfolgreichen collection.insert(&doc_id, &embedding, ...):
let extracted_entities = crate::ingestion::entities::SimpleEntityExtractor::extract(&chunk.text);
if !extracted_entities.is_empty() {
    let graph = collection.graph_index();
    let tx = /* passende TxId-Quelle, analog zum bestehenden Transaktionsmuster */;

    for entity_id in &extracted_entities {
        let entity = memfuse_core::Entity {
            id: entity_id.clone(),
            entity_type: "ExtractedTerm".into(),
            attributes: Default::default(),
        };
        if let Err(e) = graph.add_entity(tx, entity).await {
            tracing::warn!("Entity-Insert fehlgeschlagen für {:?}: {e}", entity_id);
        }
    }

    // Verknüpfe alle in diesem Chunk gefundenen Entities untereinander
    // (Ko-Okkurrenz-Kante — vereinfachte, aber nützliche Heuristik:
    // "diese Begriffe tauchen im selben Kontext auf").
    for i in 0..extracted_entities.len() {
        for j in (i + 1)..extracted_entities.len() {
            let _ = graph.add_edge(
                tx,
                extracted_entities[i].clone(),
                extracted_entities[j].clone(),
                0.5,  // Basis-Gewicht für Ko-Okkurrenz
            ).await;
        }
    }

    // WICHTIG: Auch das Dokument selbst als Entity anlegen und mit den
    // extrahierten Begriffen verknüpfen, damit die Traversal-Logik aus
    // Prompt 4 (EntityId == DocId Konvention) tatsächlich Dokumente
    // erreichen kann, nicht nur andere abstrakte Begriffe:
    let doc_entity_id = EntityId::from(doc_id.as_str());
    for term_id in &extracted_entities {
        let _ = graph.add_edge(tx, doc_entity_id.clone(), term_id.clone(), 0.8).await;
    }

    graph.commit(tx).await.ok();
}
```

**Wichtig**: Prüfe die exakte Transaktions-Konvention der bestehenden
`GraphIndex`-Trait-Methoden (`add_entity`, `add_edge`, `commit`) — die
`TxId`-Beschaffung muss konsistent mit dem Rest der Pipeline sein (evtl.
gibt es bereits einen `TxId`-Kontext aus dem umgebenden `collection.insert()`-
Aufruf, der wiederverwendet werden sollte, statt eine neue Transaktion zu
eröffnen).

### 3. Test: Ingestion erzeugt tatsächlich Graph-Entities

```rust
#[tokio::test]
async fn test_ingestion_creates_graph_entities() {
    // 1. Testdokument mit erkennbaren Entitäten einfügen
    //    ("Kunde Müller GmbH hat eine Anfrage gestellt.")
    // 2. Nach ingest_file(): graph_index.entity_count() > 0 prüfen
    // 3. Prüfen: Eine Entity mit ID "Müller GmbH" (oder ähnlich, je nach
    //    exakter Extraktion) existiert im Graph
}
```

### 4. Verifikation

```bash
cargo test -p memfuse-tauri 2>&1 | tail -40
```

Führe zusätzlich den in Prompt 17 erstellten End-to-End-Test erneut aus,
um sicherzustellen, dass die neue Entity-Extraktion keine Regression im
Gesamtpfad verursacht:
```bash
cargo test -p memfuse-tauri --test e2e_test 2>&1 | tail -20
```
```

---

## Prompt 19 — Ersteinrichtungs-Assistent (Onboarding-Screen)

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Onboarding-Flow für Erstnutzer (Zero-IT-Anspruch einlösen)

Die Strategie verspricht "Zero-IT-Setup für KMUs — ein Installer, fertig."
Aktuell muss ein Nutzer aber wissen, dass er zuerst einen Datenbankordner
wählen, dann eine Collection anlegen und dann Dokumente importieren muss —
das ist implizites Workflow-Wissen, das ein Sachbearbeiter ohne Anleitung
nicht hat. Diese Aufgabe baut einen geführten Ersteinrichtungs-Bildschirm.

### 1. Onboarding-Zustand im Frontend verwalten

**Datei**: `crates/memfuse-tauri/ui/index.html`

Füge ein Overlay hinzu, das beim allerersten Start (keine Datenbank
geöffnet) angezeigt wird:

```html
<div id="onboarding-overlay" style="
    position: fixed; inset: 0; background: rgba(15,15,25,0.95);
    display: flex; align-items: center; justify-content: center; z-index: 1000;
">
    <div style="max-width: 500px; background: #1a1a2e; padding: 2rem; border-radius: 12px; color: white;">
        <h2>👋 Willkommen bei MemFuse Brain</h2>
        <div id="onboarding-step-1">
            <p>Ihr lokaler Unternehmensassistent. Bevor Sie starten, brauchen
               wir einen Ort für Ihre Daten — sie verlassen diesen Ordner nie.</p>
            <button class="primary" id="onboarding-choose-folder" style="width: 100%; margin-top: 1rem;">
                📁 Datenordner wählen
            </button>
        </div>
        <div id="onboarding-step-2" style="display: none;">
            <p>✅ Datenordner eingerichtet.</p>
            <p>Geben Sie Ihrem ersten Wissensbereich einen Namen (z.B. "Personalwesen",
               "Verträge", "Produktdokumentation"):</p>
            <input type="text" id="onboarding-collection-name" placeholder="z.B. Personalwesen" style="width: 100%; margin-bottom: 0.5rem;">
            <button class="primary" id="onboarding-create-collection" style="width: 100%;">
                Weiter
            </button>
        </div>
        <div id="onboarding-step-3" style="display: none;">
            <p>✅ Wissensbereich "<span id="onboarding-collection-display"></span>" erstellt.</p>
            <p>Möchten Sie jetzt Dokumente importieren?</p>
            <button class="primary" id="onboarding-import-now" style="width: 100%; margin-bottom: 0.5rem;">
                📂 Ordner mit Dokumenten importieren
            </button>
            <button id="onboarding-skip" style="width: 100%; background: none; border: 1px solid #444; color: #ccc; padding: 0.5rem; border-radius: 6px;">
                Später — direkt loslegen
            </button>
        </div>
        <div id="onboarding-ollama-check" style="margin-top: 1rem; font-size: 0.8rem; opacity: 0.7;"></div>
    </div>
</div>
```

### 2. Onboarding-Logik in `app.js`

```javascript
// ── Onboarding-Flow ────────────────────────────────────────────────────
const onboardingOverlay = document.getElementById('onboarding-overlay');

async function checkOnboardingNeeded() {
    // Onboarding zeigen, solange keine Datenbank geöffnet ist
    if (!dbOpen) {
        onboardingOverlay.style.display = 'flex';
        await checkOllamaAvailability();
    } else {
        onboardingOverlay.style.display = 'none';
    }
}

async function checkOllamaAvailability() {
    const statusEl = document.getElementById('onboarding-ollama-check');
    try {
        const models = await invoke('list_ollama_models');
        if (models.length > 0) {
            statusEl.innerHTML = `✅ Ollama gefunden mit ${models.length} Modell(en)`;
        } else {
            statusEl.innerHTML = `⚠️ Ollama läuft, aber kein Modell installiert. ` +
                `Führen Sie <code>ollama pull llama3.2</code> in einem Terminal aus.`;
        }
    } catch (e) {
        statusEl.innerHTML = `⚠️ Ollama wurde nicht gefunden. Bitte installieren Sie ` +
            `Ollama von <a href="https://ollama.com" target="_blank" style="color:#6ab0ff;">ollama.com</a> ` +
            `und starten Sie es, bevor Sie fortfahren.`;
    }
}

document.getElementById('onboarding-choose-folder').addEventListener('click', async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;

    try {
        await invoke('open_database', { path: selected });
        dbOpen = true;
        dbStatusEl.textContent = `✅ Verbunden: ${selected}`;
        dbStatusEl.classList.add('connected');

        document.getElementById('onboarding-step-1').style.display = 'none';
        document.getElementById('onboarding-step-2').style.display = 'block';
    } catch (e) {
        alert(`Fehler beim Öffnen der Datenbank: ${e}`);
    }
});

document.getElementById('onboarding-create-collection').addEventListener('click', async () => {
    const name = document.getElementById('onboarding-collection-name').value.trim();
    if (!name) {
        alert('Bitte geben Sie einen Namen ein.');
        return;
    }

    try {
        await invoke('create_collection', { name });
        activeCollection = name;
        document.getElementById('onboarding-collection-display').textContent = name;
        document.getElementById('onboarding-step-2').style.display = 'none';
        document.getElementById('onboarding-step-3').style.display = 'block';
        await refreshCollections();
        selectCollection(name);
    } catch (e) {
        alert(`Collection konnte nicht erstellt werden: ${e}`);
    }
});

document.getElementById('onboarding-import-now').addEventListener('click', async () => {
    onboardingOverlay.style.display = 'none';
    document.getElementById('import-folder-btn').click();  // bestehenden Import-Flow triggern
});

document.getElementById('onboarding-skip').addEventListener('click', () => {
    onboardingOverlay.style.display = 'none';
});

// Beim App-Start aufrufen:
checkOnboardingNeeded();
```

### 3. Backend: Ollama-Erreichbarkeit früh und robust prüfen

Stelle sicher, dass `list_ollama_models()` (aus Prompt 9/10) bei
Verbindungsfehlern eine klare, nutzerverständliche Fehlermeldung liefert
(nicht nur eine technische reqwest-Fehlermeldung), da dies jetzt prominent
im Onboarding angezeigt wird:

**Datei**: `crates/memfuse-tauri/src/ollama.rs`

Prüfe, ob die bestehende Fehlermeldung in `list_models()` bereits
nutzerfreundlich ist (laut Prompt 9 sollte sie "Ist Ollama gestartet?"
enthalten). Falls nicht vorhanden, ergänze das.

### 4. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
```
```

---

## Prompt 20 — Tauri-Installer-Konfiguration für Windows/macOS/Linux

```
Du arbeitest im Repository `memfuse` (Rust Workspace).

## Aufgabe: Native Installer-Konfiguration vervollständigen

Die Strategie sieht native Installer als Hauptvertriebsweg vor ("Ein
Installer, fertig"). Diese Aufgabe vervollständigt die `tauri.conf.json`
für produktionsreife Builds auf allen drei Plattformen.

### 1. `tauri.conf.json` — Bundle-Konfiguration erweitern

**Datei**: `crates/memfuse-tauri/tauri.conf.json`

Erweitere die bestehende `bundle`-Sektion:

```json
{
  "bundle": {
    "active": true,
    "targets": "all",
    "publisher": "MemFuse Brain",
    "category": "Productivity",
    "shortDescription": "Lokaler, air-gapped KI-Assistent für Unternehmensdokumente",
    "longDescription": "MemFuse Brain durchsucht Ihre Firmendokumente (PDF, Word, E-Mails) mit einer lokalen 3-Signal-Hybridsuche und beantwortet Fragen über ein lokal laufendes Sprachmodell (Ollama) — komplett offline, DSGVO-konform, ohne Cloud-Abhängigkeit.",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "windows": {
      "webviewInstallMode": {
        "type": "downloadBootstrapper"
      },
      "nsis": {
        "displayLanguageSelector": true,
        "languages": ["German", "English"],
        "installMode": "perMachine"
      }
    },
    "macOS": {
      "minimumSystemVersion": "11.0",
      "hardenedRuntime": true,
      "entitlements": null
    },
    "linux": {
      "deb": {
        "depends": []
      },
      "appimage": {
        "bundleMediaFramework": true
      }
    }
  }
}
```

### 2. Ollama-Abhängigkeit dokumentieren (kein Bundle, da separate App)

Da Ollama eine eigenständige Anwendung ist, die der Nutzer separat
installieren muss, ergänze eine Prüfung beim App-Start (nicht nur im
Onboarding aus Prompt 19, sondern als generelle Startup-Diagnose), die
in `crates/memfuse-tauri/src/lib.rs` protokolliert, falls Ollama beim
Start nicht erreichbar ist:

```rust
// In run(), nach dem .manage(AppState::new()):
.setup(|app| {
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let bridge = crate::ollama::OllamaBridge::localhost();
        match bridge.list_models().await {
            Ok(models) if !models.is_empty() => {
                tracing::info!(count = models.len(), "Ollama erreichbar beim Start");
            }
            Ok(_) => {
                tracing::warn!("Ollama erreichbar, aber keine Modelle installiert");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Ollama beim Start nicht erreichbar");
            }
        }
    });
    Ok(())
})
```

### 3. README — Installations- und Systemanforderungen ergänzen

**Datei**: `README.md`

Ergänze einen Abschnitt:

```markdown
## Installation

### Systemanforderungen

- Windows 10/11, macOS 11+, oder eine gängige Linux-Distribution
- [Ollama](https://ollama.com) muss separat installiert und gestartet sein
  (MemFuse Brain nutzt Ollama als lokales LLM-Backend)
- Mindestens ein Ollama-Modell heruntergeladen, z.B.:
  ```bash
  ollama pull llama3.2
  ollama pull nomic-embed-text
  ```

### Installer herunterladen

Native Installer für Windows (.msi/.exe), macOS (.dmg) und Linux
(.AppImage/.deb) werden bei jedem Release unter GitHub Releases
bereitgestellt.

### Aus dem Quellcode bauen

```bash
cd crates/memfuse-tauri
cargo tauri build
```
```

### 4. GitHub Actions Release-Workflow (falls CI-Ordner existiert)

Prüfe, ob bereits ein `.github/workflows/`-Ordner existiert. Falls ja,
ergänze (oder erstelle) einen Release-Workflow für Multi-Plattform-Builds:

```yaml
# .github/workflows/tauri-release.yml
name: Tauri Release Build

on:
  push:
    tags: ["v*"]

jobs:
  build:
    strategy:
      matrix:
        platform: [macos-latest, ubuntu-22.04, windows-latest]
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Linux dependencies
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          projectPath: crates/memfuse-tauri
          tagName: ${{ github.ref_name }}
          releaseName: "MemFuse Brain ${{ github.ref_name }}"
```

### 5. Verifikation

```bash
cargo check -p memfuse-tauri 2>&1 | tail -30
# JSON-Syntax der tauri.conf.json prüfen:
python3 -c "import json; json.load(open('crates/memfuse-tauri/tauri.conf.json'))" && echo "tauri.conf.json ist valides JSON"
```

Hinweis: Ein echter `cargo tauri build` für alle drei Plattformen kann in
der Jules-Umgebung nicht vollständig verifiziert werden (benötigt
plattformspezifische Build-Tools/Cross-Compilation-Setup) — die
Konfigurationsdatei-Validität und `cargo check` sind die realistischen
Erfolgskriterien für diesen Schritt.
```

---

## Ausführungshinweise

1. **Prompt 13** (CVE) ist unabhängig und kann jederzeit zuerst laufen.
2. **Prompts 14→15→16** sind eine strikte UI-Kette — jede baut auf der
   HTML/JS-Struktur der vorherigen auf.
3. **Prompt 17** (E2E-Test) sollte NACH Prompt 7/9 (bereits erledigt) aber
   kann unabhängig von 14-16 laufen, da er das Backend, nicht die UI testet.
4. **Prompt 18** (Graph-Entities) baut auf Prompt 17 auf (nutzt dessen
   Testinfrastruktur zur Verifikation), ist aber inhaltlich unabhängig.
5. **Prompt 19** (Onboarding) setzt Prompts 14-16 voraus (nutzt deren
   UI-Elemente und Funktionen).
6. **Prompt 20** (Installer) ist der letzte Schritt und sinnvollerweise
   erst nach einer funktionierenden UI (14-16, 19) sinnvoll.

**Geschätzter Gesamtaufwand**: 12–18 Stunden Jules-Laufzeit für alle 8
neuen Prompts — deutlich weniger als Phase 2, da die Kernarchitektur steht
und "nur" noch die Nutzungsebene und Qualitätssicherung fehlen.
