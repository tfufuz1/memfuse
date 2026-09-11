# APM-38 Replay-Schutz-Analyse für memfuse-mcp (stdio JSON-RPC)

**Datum:** 2026-09-11
**Target:** `crates/memfuse-mcp/` (Protokoll-/Transport-Layer)
**Status:** BESTANDEN (Zustandslos-Idempotent / Downstream `tx_id`/`seq_no` HMAC-gebunden)
**Prüfkontext:** APM-38 dritter Pfad (WAL-Replay: [VERIFIZIERT], Checkpoint-Restore: [VERIFIZIERT], MCP-stdio-Request-Handling: [VERIFIZIERT])

---

## 1. Übersicht & Zielsetzung

Gemäß APM-38 verlangt das Sicherheitskonzept von MemFuse eine explizite Prüfung aller drei Angriffs-/Replay-Pfade:
1. **WAL-Replay:** Verifiziert in `crates/memfuse-store/src/wal.rs` über `seq_no`/`tx_id`-Bindung und HMAC-SHA256 Signatur.
2. **Checkpoint-Restore:** Verifiziert in `memfuse-checkpoint`.
3. **MCP stdio Request-Handling:** Verifiziert in diesem Audit (`crates/memfuse-mcp`).

Die APM-38 Vorgabe untersagt das implizite Annehmen von Protokoll-Zustandslosigkeit. Jede Behauptung bezüglich "zustandslos-idempotent" muss den tatsächlichen Codefluss aller Request-Handler analysieren und im Code sowie in der Audit-Dokumentation belegen.

---

## 2. Handler-Analyse & Idempotenz-Matrix

| Request / Tool | Operationstyp | Zustand im MCP-Server | Downstream-Wirkung (`memfuse-db` / `memfuse-store`) | Replay-Sicherheit & Idempotenz-Kette |
| :--- | :--- | :--- | :--- | :--- |
| `initialize` | Read/Echo | Zustandslos | Keine DB-Operation | **Idempotent:** Liefert statische MCP Capabilities DTO zurück. |
| `initialized` | Notification | Zustandslos | Keine DB-Operation | **Idempotent:** Leere Bestätigungs-Response (`{}`). |
| `tools/list` | Read | Zustandslos | Keine DB-Operation | **Idempotent:** Statische Tool-Metadaten Liste. |
| `ping` | Read | Zustandslos | Keine DB-Operation | **Idempotent:** Health-Check Echo (`{}`). |
| `memfuse_search` | Read | Zustandslos | Read-Only Hybrid-Query | **Idempotent:** Lesezugriff auf Vektor-/BM25-Indices, keine Zustandsänderung. |
| `memfuse_get` | Read | Zustandslos | Read-Only Document Lookup | **Idempotent:** Lesezugriff per ID, keine Zustandsänderung. |
| `memfuse_collections` | Read | Zustandslos | Read-Only Collection List | **Idempotent:** Lesezugriff, keine Zustandsänderung. |
| `memfuse_insert` | Write | Zustandslos | `col.insert(chunk_id, embedding, meta)` | **Idempotent via Determinismus & Storage-HMAC:** (Siehe Detailanalyse unten). |

---

## 3. Detailanalyse des schreibenden Handlers (`memfuse_insert`)

1. **Write-Authorization Guard & Sandbox Policy:**
   - DB-Schreibzugriffe sind standardmäßig gesperrt (`allow_db_writes: false` in `SandboxPolicy`).
   - Schreiboperationen erfordern explizite Opt-In Freigabe (`MEMFUSE_MCP_ALLOW_WRITE=true` oder `--allow-write`).

2. **Deterministische Schlüssel-Ableitung in Downstream `memfuse-db`:**
   - `memfuse_insert` verarbeitet Anfragen mit fester `id` (oder auto-generated Chunk-IDs nach dem Schema `{id}:chunk:{i}`).
   - `col.insert()` leitet aus dem String `id` deterministisch die `DocId` ab (`DocId::from_key(id)`).
   - Die Speicherschlüssel `user_key` (`key_type=0`, basierend auf den Bytes von `id`) und `doc_key` (`key_type=1`, basierend auf `DocId`) sind strikt deterministisch.

3. **Verhalten bei Replay eines identischen `memfuse_insert` Requests:**
   - Ein erneut eingespielter Request überschreibt in LSM-Storage (`memfuse-store`) und Vektor-Index (`memfuse-index`) die KV-Einträge von `user_key` und `doc_key` mit identischen Daten.
   - Es finden **keine** sequenziellen Seiteneffekte statt (kein Hochzählen von Zählern, kein Abbuchen von Guthaben/Budgets, keine externen RPC-Calls).
   - Das Endergebnis des Systemzustands nach $1$ Aufruf entspricht exakt dem Zustand nach $N$ identischen Aufrufen (strikte Idempotenz $f(x) = f(f(x))$).

4. **Krypto- & WAL-Integrität in `memfuse-store`:**
   - Jede durch `col.insert()` ausgelöste LSM-Transaktion schreibt WAL-Records, die mit `seq_no` und `tx_id` HMAC-SHA256 verifiziert sind (`crates/memfuse-store/src/wal.rs`).
   - Ein Angreifer kann über den stdio-Kanal keine fremden Transaktions-IDs oder gefälschten WAL-Einträge einschleusen.

5. **Abwesenheit von Session- & Transaktions-Zustand im MCP-Server:**
   - `McpServer` speichert keine uncommitted Multi-Request Transaktions-Handles, keine Auth-Tokens mit Ablauf-Fenstern und keine ephemeren Sequence-Counter über stdio-Aufrufe hinweg.
   - Der stdio-Loop (`run_stdio`) liest zeilenweise unabhängige JSON-RPC 2.0 Requests aus `stdin`.

---

## 4. Fazit & APM-38 Abnahmestatus

Die Replay-Schutz-Analyse für `memfuse-mcp` bestätigt **Option A**:
- Das Protokoll ist zustandslos-idempotent.
- Alle zustandsverändernden Downstream-Operationen sind über ihre deterministische Key-Ableitung idempotent und über `memfuse-store` `tx_id`/`seq_no`-gebunden HMAC-verifiziert.
- `memfuse-mcp` selbst benötigt keinen zusätzlichen Sequence-/Nonce-Tracking-Filter.
- Inline-Dokumentation wurde in `crates/memfuse-mcp/src/lib.rs` hinterlegt (`AI-TAG[SMELL][RESOLVED] audit-APM-38-mcp`).

Damit sind **alle drei Pfade von APM-38** (WAL-Replay, Checkpoint-Restore, MCP-stdio) explizit geprüft, verifiziert und dokumentiert. APM-38 ist vollständig geschlossen.
