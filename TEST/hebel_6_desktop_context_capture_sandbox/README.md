# Hebel 6: Desktop Context Capture & Secure Sandbox (aus TextForge & Template-Tauri)

## 1. Ausgangslage & Optimierungspotenzial für MemFuse
MemFuse ist das universelle Context- und Memory-OS für KI-Agenten und Power-User. Aktuell müssen Dokumente oder Notizen entweder explizit via CLI, API oder Datei-Ingest in MemFuse geladen werden.

Aus **TextForge** und **Template-Tauri** können zwei bahnbrechende Desktop-Fähigkeiten direkt für `memfuse-tauri` übernommen werden:

### 1. Quell-App-Erkennung & Automatischer Kontext-Ingest (`source_app.rs` + `clipboard_monitor.rs`)
- Wenn ein Nutzer Text oder Code kopiert, überwacht ein nativer Hintergrund-Thread die Zwischenablage (event-basiert über Wayland oder Polling über X11).
- Über KDE Plasma 6 DBus (`org.kde.KWin /KWin`) oder X11 (`_NET_ACTIVE_WINDOW`) wird die **aktive Quell-Anwendung** (z. B. "Google Chrome", "VS Code", "Slack", "Ghostty Terminal") ermittelt.
- **Synergie für MemFuse:** Jedes kopierte Snippet kann als `ContextChunk` mit Metadaten (`source_app: "VS Code"`, `window_title: "lib.rs"`, `timestamp`) automatisch im Hintergrund indiziert werden. Der Nutzer hat sofort ein lückenloses Gedächtnis all seiner Arbeitsaktivitäten!

### 2. Sichere QuickJS-Sandbox für benutzerdefinierte Daten-Filter (`quickjs_sandbox.rs`)
- Ermöglicht es Nutzern und Agenten, benutzerdefinierte JavaScript-Transformationen, Regex-Normalisierungen oder Datenbereinigungen vor dem Ingest auszuführen.
- **Sicherheits-Garantie:** `rquickjs` mit strengem Timeout (Standard 3s) und Input/Output-Byte-Limits (2 MB / 512 KB), um Endlosschleifen und Heap-Überläufe vollständig zu isolieren.
- Löst direkt die in `memfuse-core` bereits vorgemerkten Fehlervarianten `MemFuseError::Sandbox` und `MemFuseError::SandboxTimeout` ein.

## 2. Extrahierte Komponenten

| Datei | Quelle | Beschreibung |
|:---|:---|:---|
| [`source_app.rs`](./source_app.rs) | `textforge/src-tauri/src/clipboard/source_app.rs` | Quell-App-Erkennung via KDE DBus, Wayland & X11 |
| [`clipboard_monitor.rs`](./clipboard_monitor.rs) | `textforge/src-tauri/src/clipboard/mod.rs` | Event-basierte Zwischenablagen-Überwachung mit Debouncing |
| [`quickjs_sandbox.rs`](./quickjs_sandbox.rs) | `textforge/src-tauri/src/sandbox/mod.rs` | Isolierte QuickJS Sandbox mit Timeout-Guard |
| [`tauri_system_api.md`](./tauri_system_api.md) | `template-tauri/tauri_complete_system_api.md` | Umfassende Dokumentation aller nativen Desktop-APIs |
| [`memfuse_clipboard_ingestion.rs`](./memfuse_clipboard_ingestion.rs) | Neu erstellt | Tauri-Kommando zur direkten Ingestion in eine MemFuse Collection |

## 3. Implementierungsplan für MemFuse
1. Füge `source_app.rs` und `clipboard_monitor.rs` zu `crates/memfuse-tauri/src/services/` hinzu.
2. Ermögliche in den Tauri-Einstellungen einen Schalter *"Auto-Context Ingestion"*.
3. Sobald Text kopiert wird, ruft der Monitor `collection.insert()` mit der erkannten Quell-App und Fensterüberschrift als Metadaten auf.
