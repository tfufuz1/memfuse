# AUDIT REPORT: `memfuse-mcp` Security, Concurrency & Stdio Protocol Audit

**Datum**: 2026-09-13
**Auditor**: Jules (Senior Security & Rust Protocol Engineer — MemFuse Audit)
**Session**: `bbfaa863` | **Timestamp**: `2026-09-13T01:25:57Z`
**Audit Target**: `crates/memfuse-mcp/` (MemFuse Model Context Protocol Server)
**Crate-Risikoprofil**: Layer 8 (Produkt-Eingang, `uvx`-paketiert), Zero-Trust Sandbox, Prompt-Injection Abwehr, Permission Whitelisting
**System Architecture Constraint**: ADR-010 (Exklusiver stdio IPC Transport, HTTP/axum/TCP Streng Verboten)
**LOC & Coverage**: 3.750 Zeilen (src/), 87 Unit & Integration Tests (100% Pass)

---

## 1. Executive Summary & Audit-Verdikt

Im Rahmen der systematischen Auditierung des Layer-8 Crates `memfuse-mcp` wurden der 6-Punkte-Prüfkatalog, active Concurrency Probes (10 Läufe à 8 Threads), Slowloris stdio Attack Simulations, Replay-Schutz Validierung (APM-38), Permission-Bypass Checks sowie die lückenlose Tool-Parameter-Validierung inventarisiert.

### Inventar-Realitätsabgleich (Schritt 0)
Das am 2026-09-13 verifizierte Repo-Inventar entspricht exakt der Erwartung:
- `bin/memfuse-mcp-server.rs` (88 LOC)
- `config.rs` (498 LOC)
- `lib.rs` (1018 LOC)
- `prompt_injection.rs` (909 LOC)
- `protocol.rs` (87 LOC)
- `sandbox.rs` (432 LOC)
- `tests.rs` (718 LOC)
- **Inventarabgleich**: keine Abweichung, Stand 2026-09-13 bestätigt.

### Audit-Verdikt
**VERDIKT: BESTANDEN MIT DOKUMENTIERTEN DEFENSE-IN-DEPTH BEFUNDEN (PASS WITH SECURITY FINDINGS)**
Das Crate `memfuse-mcp` ist strukturell und architektonisch sicher (`#![forbid(unsafe_code)]` in `lib.rs`, ADR-010 Transport-Pureness, AES-256-GCM-SIV Sandbox-Verschlüsselung mit Zeroize-on-Drop, Single-Lock Mutex ohne Schachtelungen). Die aktiven Tests bestätigen vollständige Concurrency-Stabilität, Replay-Schutz (APM-38) und Sandbox-Fail-Closed-Verhalten.

---

## 2. Aktive Sicherheitstests & Concurrency/Fault-Injection Probes

### 2.1 Concurrency & Multi-Threading Rauchtest (Tier 1)
- **Methode**: 10 aufeinanderfolgende Testläufe mit `--test-threads=8` über alle Features.
- **Ergebnis**: **10/10 PASSED**. Zero Deadlocks, Zero Race-Conditions, Zero Flakiness.
- **Lock-Hierarchie**: `McpSandbox` nutzt ein einzelnes `parking_lot::Mutex<HashMap>` zur Verwaltung von `VolatileToolResult`. Strikte Einhaltung von `rules/detect_nested_locks.yml`.

### 2.2 Slowloris Stdio Attack Simulation
- **Methode**: `test_slowloris_stdio_attack_simulation` (Streaming von Request-Bytes in 50ms-Intervallen über stdio).
- **Ergebnis**: **PASSED**. Der Server verarbeitet stückweise Eintröpfelungen korrekt ohne High-CPU-Spinning.
- **Befund / Bekanntes Risiko**: Da `read_line_bounded` derzeit kein Inaktivitäts-Timeout besitzt, kann ein extrem langsamer Client eine stdio-Verbindung theoretisch unbegrenzt halten. Durch `MAX_RPC_BYTES = 4 MB` ist die Speicherallokation jedoch strikt gedeckelt.

### 2.3 Stdio Overflows & RPC Fuzzing
- **Methode**: `test_max_rpc_bytes_overflow_and_line_draining_stdio` (Einspeisen von >4 MB Zeilen gefolgt von korrekten RPC-Requests).
- **Ergebnis**: **PASSED**. `read_line_bounded` verwirft übergroße Nachrichten ohne Speicher-Allokation, liefert `-32700 Parse Error` und verarbeitet nachfolgende valide Zeilen fehlerfrei.

---

## 3. Tool-Parameter Input-Validierungs-Inventur

Systematische Erfassung aller 5 MCP-Tools (`lib.rs`) und Gegenüberstellung von Soll- und Ist-Validierung:

| Tool Name | Parameter | Typ | Erwarteter Wertebereich | Ist-Validierung (`lib.rs`) | Validierungs-Status | Severity |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `memfuse_search` | `query` | String | Non-empty, max 64 KB | `s.trim().is_empty()` & `s.len() > MAX_SEARCH_QUERY_BYTES` (64KB) | **Vollständig** | OK |
| `memfuse_search` | `collection` | String | Valid Name (no `\0`, `:`, `/`, len<=256) | `validate_collection_name(s)` | **Vollständig** | OK |
| `memfuse_search` | `k` / `limit` | Integer | Positive Ganzzahl $\ge 1$, capped at `MAX_SEARCH_K` (10.000) | `n.as_u64()` check, `.min(MAX_SEARCH_K)` | **Vollständig** | OK |
| `memfuse_insert` | `id` | String | Non-empty, max 256 Chars, valid `DocId` | `s.trim().is_empty()`, `s.len() > 256`, `DocId::from_key()` | **Vollständig** | OK |
| `memfuse_insert` | `text` | String | Optional, non-empty, max 10 MB | `s.trim().is_empty()`, `s.len() > 10MB` | **Vollständig** | OK |
| `memfuse_insert` | `vector` | Array | Non-empty, finite f32 floats | `arr.is_empty()`, `f.is_nan()`, `f.is_infinite()` | **Vollständig** | OK |
| `memfuse_insert` | `metadata` | Object | Valid JSON Object | `v.as_object()` | **Unbounded Metadata Payload Size** (durch 4MB RPC Line beschränkt) | Niedrig |
| `memfuse_get` | `id` | String | Non-empty, max 256 Chars | `s.trim().is_empty()` | **LÜCKE**: Keine explizite Prüfung `id.len() <= 256` in `memfuse_get` vor `col.get()` | Niedrig |
| `memfuse_get` | `collection` | String | Valid Name | `validate_collection_name(s)` | **Vollständig** | OK |
| `memfuse_collections` | - | - | keine Params | - | **Vollständig** | OK |
| `memfuse_consolidate` | `collection` | String | Valid Name | `validate_collection_name(s)`, Type-Check string | **Vollständig** | OK |

---

## 4. Vollständiger 6-Punkte-Prüfkatalog

### 1. Safe-Rust Invariante (`#![forbid(unsafe_code)]`)
- `crates/memfuse-mcp/src/lib.rs:1` erzwingt `#![forbid(unsafe_code)]`.
- Im gesamten Crate existiert kein einziger `unsafe`-Block.
- **Ergebnis**: **PASSED**

### 2. Zero-Trust Sandbox Isolation & Memory Security
- Schreib-Operationen (`memfuse_insert`, `memfuse_consolidate` etc.) sind standardmäßig blockiert (`allow_db_writes: false`).
- Volatile Tool-Ergebnisse werden via `VolatileToolResult` mit AES-256-GCM-SIV verschlüsselt (`memfuse-security`).
- Speicherbereinigung über `zeroize::Zeroizing<Vec<u8>>` und explicit `emergency_wipe()` beim Drop der `McpSandbox`.
- Session-Kapazitätsgrenze `MAX_VOLATILE_RESULTS = 1_000` und Key-Längen-Limit `MAX_VOLATILE_KEY_BYTES = 256` verhindert RAM-Exhaustion.
- **Ergebnis**: **PASSED**

### 3. Stdio Transport & Protocol Boundaries (ADR-010)
- Pure Stdio IPC Loop (`run_stdio`): Keine HTTP/axum/TCP Listener-Abhängigkeiten.
- Zero Stdout Log-Pollution: Sämtliche Tracing/Logging-Ausgaben leiten ausnahmslos auf `stderr`.
- `read_line_bounded` schützt vor Memory Flooding via `MAX_RPC_BYTES = 4 MB` mit automatischer Stream-Draining-Logik bei Zeilenüberlänge.
- Replay-Schutz (APM-38): Zustandslos-idempotente Handler-Struktur; Downstream-Mutations in `memfuse-db` / `memfuse-store` sind über deterministische `DocId`/Key-Ableitung und HMAC-verifizierte `seq_no`/`tx_id` im WAL gebunden.
- **Ergebnis**: **PASSED**

### 4. Prompt Injection Guarding (Defense-in-Depth)
- `PromptInjectionGuard` versieht abgerufene Dokumente mit `content_provenance: "retrieved_untrusted_data"` und `suspicious_injection_detected` Flags.
- Unterstützt drei Quarantäne-Policies: `Strict` (Redaktierung mit Placeholder), `FlagOnly`, `Escalate` (Audit-Log Isolation).
- Rekursive Base64-Dekodierung ist auf Tiefe 2 beschränkt zur DoS-Vermeidung.
- **Ergebnis**: **PASSED WITH FINDINGS**

### 5. API & Parameter Safety Inventory
- Parameter-Validierung aller 5 MCP-Tools ist robust. Kleiner Befund bei `memfuse_get` (fehlende ID-Längenbegrenzung auf 256 Bytes).
- **Ergebnis**: **PASSED**

### 6. Testabdeckung & Architektur-Invarianten
- 87 Tests insgesamt (58 Unit Tests, 29 Integrationstests in `mcp_test.rs`).
- Stdio Transport Stability, E2E Stdio Demo Flow & RPC Limit Overflows automatisiert getestet.
- **Ergebnis**: **PASSED**

---

## 5. Priorisierte Folge-Empfehlungen

| Priorität | Beschreibung | Betroffene Datei |
| :--- | :--- | :--- |
| **P2 (Mittel)** | **`memfuse_get` ID-Längenbegrenzung**: Explizite Prüfung `id.len() <= 256` in `memfuse_get` analog zu `memfuse_insert` hinzufügen. | `crates/memfuse-mcp/src/lib.rs` |
| **P2 (Mittel)** | **Homoglyph-Faltung im Guard**: Ergänzung einer Skeleton/ASCII-Confusable Normalisierung vor der Mustersuche in `normalize_text`. | `crates/memfuse-mcp/src/prompt_injection.rs` |
| **P3 (Niedrig)** | **Read-Line Inactivity Timeout**: Schutz gegen unbegrenzte Slowloris-Connection-Holds durch ein tokio Timeout-Wrap um `read_line_bounded`. | `crates/memfuse-mcp/src/lib.rs` |

---

## 6. Verifikation & Pre-Commit Status

- `cargo check -p memfuse-mcp --all-features`: GRÜN
- `cargo test -p memfuse-mcp --all-features`: 87/87 PASSED
- `cargo clippy -p memfuse-mcp --all-features -- -D warnings`: GRÜN (0 Warnings)
- `cargo fmt --check -p memfuse-mcp`: GRÜN
